use crate::{policy::*, state::*};
use core::sync::atomic::Ordering;
use device_core::{
    app::*,
    geometry::Rotation,
    settings::{ConfigurationId, Settings, SLOT_BYTES},
    telemetry::*,
};
use device_ui::Framebuffer;
use e1004_bsp::storage::{SlotStorage, StorageError};
use embassy_futures::select::{select, Either};
use embassy_time::{Duration, Instant, Timer};

const SLEEP_RETRY_MS: u64 = 100;
const QUIESCE_TIMEOUT_MS: u64 = 2000;
const ERASED_SETTINGS: [u8; SLOT_BYTES] = [0xff; SLOT_BYTES];

fn sleep_permitted(system: &System, ticket: SleepTicket, armed: bool) -> bool {
    system.snapshot().is_some_and(|app| {
        SleepConditions {
            app,
            ticket,
            buttons_armed: armed,
            outputs_quiet: true,
            radios_quiet: system.radios_quiet(),
            bus_empty: system.bus.is_empty(),
            sleep_pending: system.sleep_request.signaled(),
            events_empty: system.events.is_empty(),
        }
        .permitted()
    })
}

pub trait BusIo: t133a01::PanelIo {
    fn stop(&mut self);
    fn sd_diagnose(&mut self) -> StorageStatus;
    fn sd_import(
        &mut self,
        _: device_core::media::ImageId,
        _: &mut [u8],
    ) -> impl core::future::Future<Output = Result<device_core::media::ImageDescriptor, StorageError>>
    {
        core::future::ready(Err(StorageError::Absent))
    }
    fn deep_sleep(&mut self, seconds: u32) -> SleepOutcome;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SleepOutcome {
    Entered,
    Rejected,
}

pub fn boot_settings(flash: &mut impl SlotStorage) -> (Settings, Option<Fault>) {
    let first = read_slot(flash, 0);
    let second = read_slot(flash, 1);
    let (settings, error) = loaded_settings(
        first.as_ref().unwrap_or(&ERASED_SETTINGS),
        second.as_ref().unwrap_or(&ERASED_SETTINGS),
    );
    (
        settings,
        if first.is_err() || second.is_err() {
            Some(Fault::SettingsStorage)
        } else {
            error
        },
    )
}
fn read_slot(flash: &mut impl SlotStorage, slot: usize) -> Result<[u8; SLOT_BYTES], StorageError> {
    let mut bytes = ERASED_SETTINGS;
    flash.read(slot, &mut bytes)?;
    Ok(bytes)
}

pub async fn application_loop(
    system: &System,
    settings: Settings,
    error: Option<Fault>,
    wake: WakeReason,
    memory: impl Fn() -> (u32, u32),
) {
    let mut app = App {
        refresh_seconds: settings.refresh_seconds,
        next_refresh_ms: u64::from(settings.refresh_seconds) * MILLISECONDS_PER_SECOND,
        rotation: match settings.rotation {
            0 => Rotation::Portrait,
            1 => Rotation::Landscape,
            2 => Rotation::InvertedPortrait,
            _ => Rotation::InvertedLandscape,
        },
        screen: settings.wake_screen(),
        resume: settings.resume,
        persisted_settings: settings,
        daily: settings.daily,
        selected_task: settings.selected_task,
        calendar_month: settings.calendar_month,
        gallery: device_core::media::Gallery::loading(settings.gallery),
        ..App::default()
    };
    app.diagnostics.last_error = error;
    let mut event = AppEvent::Wake(wake);
    let mut pending_bus = None;
    loop {
        let transition = app.transition(event);
        app = transition.state;
        let (heap, psram) = memory();
        app.diagnostics.heap_bytes = heap;
        app.diagnostics.psram_bytes = psram;
        system.publish(app);
        if let AppEvent::ConfigurationSaved { id, .. } = event {
            system.acknowledge_configuration(id);
        }
        for action in transition.actions {
            match action {
                Action::None => {}
                Action::Measure => system.measure.signal(()),
                Action::Display { id } => pending_bus = Some(BusRequest::Display { app, id }),
                Action::StopPanel => {
                    system.cancel_display();
                    system.stop_panel.signal(());
                    pending_bus = None;
                }
                Action::Sleep { seconds } => {
                    system.sleep_request.signal(SleepTicket {
                        sequence: app.sequence,
                        seconds,
                    });
                }
                Action::Log(fault) => system.record(LogRecord::Fault(fault)),
            }
        }
        crate::effect_dispatch::dispatch(&mut pending_bus, |request| {
            system.bus.try_send(request).err().map(|error| match error {
                embassy_sync::channel::TrySendError::Full(request) => request,
            })
        });
        event = system.events.receive().await;
    }
}

pub async fn ticks(system: &System) {
    loop {
        Timer::after_secs(1).await;
        system
            .events
            .send(AppEvent::Tick(Instant::now().as_millis()))
            .await;
    }
}

pub async fn panel_update(
    system: &System,
    panel: &mut t133a01::Panel,
    hardware: &mut impl BusIo,
    bytes: &[u8],
) -> Result<(), Fault> {
    let operation = async {
        panel.initialize(hardware).await?;
        panel.display(hardware, bytes).await
    };
    let result = match select(
        Timer::after_millis(DISPLAY_TIMEOUT_MS),
        select(system.stop_panel.wait(), operation),
    )
    .await
    {
        Either::First(()) | Either::Second(Either::First(())) => Err(t133a01::Error::BusyTimeout),
        Either::Second(Either::Second(result)) => result,
    };
    if result.is_err() {
        hardware.stop();
        *panel = t133a01::Panel::default();
    }
    result.map_err(crate::policy::panel_fault)
}

async fn quiesce(system: &System) -> bool {
    system.button_armed.reset();
    system.output_quiet.reset();
    system.button_sleep.signal(true);
    system.output_sleep.signal(true);
    let preparation = async {
        if !system.button_armed.wait().await {
            return false;
        }
        system.output_quiet.wait().await;
        if system.capabilities.wifi {
            system.wifi_command.send(WifiCommand::Quiesce).await;
        }
        if system.capabilities.ble {
            system.ble_enabled.signal(false);
        }
        while !system.radios_quiet() {
            system.radio_changed.wait().await;
        }
        system.output_quiet.reset();
        system.output_sleep.signal(true);
        system.output_quiet.wait().await;
        true
    };
    match select(Timer::after_millis(QUIESCE_TIMEOUT_MS), preparation).await {
        Either::First(()) => {
            system.record(LogRecord::Event("sleep_preparation_timeout"));
            false
        }
        Either::Second(armed) => armed,
    }
}

pub async fn bus_loop(
    system: &System,
    hardware: impl BusIo,
    flash: impl SlotStorage,
    frame: &mut [u8],
) {
    bus_loop_with_media(system, hardware, flash, frame, &mut []).await;
}

pub async fn bus_loop_with_media(
    system: &System,
    mut hardware: impl BusIo,
    mut flash: impl SlotStorage,
    frame: &mut [u8],
    image: &mut [u8],
) {
    use device_core::media::{GalleryEvent, PackedFrame};
    use e1004_bsp::media::{MediaCommand, MediaError, MediaStore};
    let opened = MediaStore::open(&mut flash);
    let mut media_opened = opened.is_ok();
    let mut media = opened.unwrap_or_default();
    if let Some(bytes) = flash.read_retained_gallery() {
        if let Some((settings, _)) = device_core::settings::decode(&bytes) {
            system.events.send(AppEvent::ResumeGallery(settings)).await;
        }
    }
    system
        .events
        .send(AppEvent::Gallery(GalleryEvent::Ready(media.catalog())))
        .await;
    let mut media_deadline = None;
    let mut panel = t133a01::Panel::default();
    let mut panel_sleeping = false;
    let mut resume_attempt = None;
    loop {
        let incoming = select(system.bus.receive(), system.sleep_request.wait());
        let incoming = if let Some(deadline) = media_deadline {
            match select(Timer::at(deadline), incoming).await {
                Either::First(()) => {
                    media.abort();
                    media_deadline = None;
                    system
                        .events
                        .send(AppEvent::Gallery(GalleryEvent::Ready(media.catalog())))
                        .await;
                    system.record(LogRecord::Event("image_transfer_expired"));
                    continue;
                }
                Either::Second(request) => request,
            }
        } else {
            incoming.await
        };
        let request = match incoming {
            Either::First(request) => request,
            Either::Second(ticket) => BusRequest::Sleep(ticket),
        };
        match request {
            BusRequest::Display { app, id } => {
                if !system
                    .snapshot()
                    .is_some_and(|current| display_is_current(current, id))
                {
                    continue;
                }
                if app.screen == ScreenId::Gallery && !media_opened {
                    if let Ok(recovered) = MediaStore::open(&mut flash) {
                        media = recovered;
                        media_opened = true;
                        system
                            .events
                            .send(AppEvent::Gallery(GalleryEvent::Busy(true)))
                            .await;
                        system
                            .events
                            .send(AppEvent::Gallery(GalleryEvent::Ready(media.catalog())))
                            .await;
                        continue;
                    }
                }
                system.stop_panel.reset();
                system.start_display(id, app);
                let result = match Framebuffer::new(frame, app.rotation) {
                    Ok(mut target) => {
                        let loaded = if app.screen == ScreenId::Gallery && !media_opened {
                            Err(Fault::Storage)
                        } else if app.screen == ScreenId::Gallery {
                            app.gallery
                                .view()
                                .image
                                .map(|(id, expected)| {
                                    let descriptor = media
                                        .load(&mut flash, id, image)
                                        .map_err(|_| Fault::Storage)?;
                                    if descriptor != expected {
                                        return Err(Fault::Storage);
                                    }
                                    PackedFrame::new(descriptor.rotation, image)
                                        .ok_or(Fault::Storage)
                                })
                                .transpose()
                        } else {
                            Ok(None)
                        };
                        match loaded {
                            Ok(source) => {
                                device_ui::render_with_media(&app, &mut target, source);
                                panel_sleeping = false;
                                panel_update(system, &mut panel, &mut hardware, frame).await
                            }
                            Err(fault) => Err(fault),
                        }
                    }
                    Err(_) => Err(Fault::Bus),
                };
                system.finish_display(id, result);
                system.record(LogRecord::Event(if result.is_ok() {
                    "display_complete"
                } else {
                    "display_failed"
                }));
                system
                    .events
                    .send(AppEvent::DisplayFinished { id, result })
                    .await;
                if result.is_ok() {
                    system.events.send(AppEvent::RequestSleep).await;
                }
            }
            BusRequest::Sleep(ticket) => {
                if !system.snapshot().is_some_and(|app| ticket.current(app)) {
                    continue;
                }
                let armed = quiesce(system).await;
                let permitted = sleep_permitted(system, ticket, armed);
                if !permitted {
                    system.record(LogRecord::Event("sleep_deferred"));
                }
                if permitted {
                    if let Some(app) = system.snapshot() {
                        let desired = app.settings();
                        if desired != app.persisted_settings && resume_attempt != Some(desired) {
                            resume_attempt = Some(desired);
                            save_settings(system, &mut flash, desired, None).await;
                            system.button_sleep.signal(false);
                            system.output_sleep.signal(false);
                            system.events.send(AppEvent::RequestSleep).await;
                            continue;
                        }
                    }
                    if !panel_sleeping {
                        let sleep = select(
                            Timer::after_millis(QUIESCE_TIMEOUT_MS),
                            panel.sleep(&mut hardware),
                        )
                        .await;
                        if !matches!(sleep, Either::Second(Ok(()))) {
                            let fault = match sleep {
                                Either::Second(Err(t133a01::Error::Transport)) => Fault::Bus,
                                _ => Fault::PanelTimeout,
                            };
                            hardware.stop();
                            system.record(LogRecord::Event("panel_sleep_failed"));
                            system
                                .events
                                .send(AppEvent::PanelFailed {
                                    id: ticket.sequence,
                                    fault,
                                })
                                .await;
                        } else {
                            panel_sleeping = true;
                        }
                    }
                    if panel_sleeping && sleep_permitted(system, ticket, armed) {
                        if let Some(app) = system.snapshot() {
                            let retained = Settings {
                                gallery: app.gallery.settings(),
                                ..app.settings()
                            };
                            flash.retain_gallery(&device_core::settings::encode(retained, 0));
                        }
                        hardware.stop();
                        system.sleeping.store(true, Ordering::Release);
                        if hardware.deep_sleep(ticket.seconds) == SleepOutcome::Entered {
                            core::future::pending::<()>().await;
                        }
                        system.sleeping.store(false, Ordering::Release);
                    }
                }
                system.button_sleep.signal(false);
                system.output_sleep.signal(false);
                Timer::after(Duration::from_millis(SLEEP_RETRY_MS)).await;
                system.events.send(AppEvent::RequestSleep).await;
            }
            BusRequest::Save(settings) => {
                save_settings(system, &mut flash, settings, None).await;
            }
            BusRequest::Configure { id, settings } => {
                save_settings(system, &mut flash, settings, Some(id)).await;
            }
            BusRequest::Diagnose => {
                system.events.send(AppEvent::StorageBusy(true)).await;
                let status = hardware.sd_diagnose();
                system.events.send(AppEvent::StorageChanged(status)).await;
                system.events.send(AppEvent::StorageBusy(false)).await;
            }
            BusRequest::Media(request) => {
                let command = request.command;
                if !media_opened {
                    if let Ok(recovered) = MediaStore::open(&mut flash) {
                        media = recovered;
                        media_opened = true;
                    }
                }
                let media_available =
                    media_opened && image.len() == device_core::geometry::FRAME_BYTES;
                let committed = match command {
                    MediaCommand::Commit(_) => {
                        media.transfer().map(|transfer| transfer.record().id())
                    }
                    MediaCommand::Import(id) => Some(id),
                    _ => None,
                };
                let result = if !media_available {
                    Err(MediaError::Storage(StorageError::Absent))
                } else {
                    match command {
                        MediaCommand::Begin {
                            transfer,
                            image,
                            rotation,
                            checksum,
                        } => {
                            if frame.len() != device_core::geometry::FRAME_BYTES {
                                Err(MediaError::Corrupt)
                            } else {
                                media
                                    .begin(&mut flash, transfer, image, rotation, checksum)
                                    .map(|()| 0)
                            }
                        }
                        MediaCommand::Data {
                            transfer,
                            offset,
                            data,
                        } => media
                            .write(&mut flash, transfer, offset, data.bytes())
                            .map(|()| offset + data.bytes().len()),
                        MediaCommand::Commit(id) => media
                            .commit(&mut flash, id)
                            .map(|()| device_core::geometry::FRAME_BYTES),
                        MediaCommand::Abort(id) => {
                            if media.transfer().is_some_and(|active| active.id() != id) {
                                Err(MediaError::Transfer(
                                    device_core::media::TransferError::Identity,
                                ))
                            } else {
                                media.abort();
                                Ok(0)
                            }
                        }
                        MediaCommand::Delete(id) => media.delete(&mut flash, id).map(|()| 0),
                        MediaCommand::Import(id) => {
                            if media.transfer().is_some() {
                                Err(MediaError::Busy)
                            } else {
                                match hardware.sd_import(id, image).await {
                                    Ok(descriptor) => {
                                        system
                                            .events
                                            .send(AppEvent::StorageChanged(StorageStatus::Ready))
                                            .await;
                                        let imported = async {
                                            if device_core::settings::crc32(image)
                                                != descriptor.checksum
                                            {
                                                return Err(MediaError::Corrupt);
                                            }
                                            media.begin(
                                                &mut flash,
                                                0,
                                                id,
                                                descriptor.rotation,
                                                descriptor.checksum,
                                            )?;
                                            for (index, chunk) in image
                                                .chunks(device_core::media::CHUNK_BYTES)
                                                .enumerate()
                                            {
                                                media.write(
                                                    &mut flash,
                                                    0,
                                                    index * device_core::media::CHUNK_BYTES,
                                                    chunk,
                                                )?;
                                                embassy_futures::yield_now().await;
                                            }
                                            media.commit(&mut flash, 0)?;
                                            Ok(device_core::geometry::FRAME_BYTES)
                                        }
                                        .await;
                                        if imported.is_err() {
                                            media.abort();
                                        }
                                        imported
                                    }
                                    Err(error) => {
                                        system
                                            .events
                                            .send(AppEvent::StorageChanged(
                                                if error == StorageError::Absent {
                                                    StorageStatus::Absent
                                                } else {
                                                    StorageStatus::Fault
                                                },
                                            ))
                                            .await;
                                        Err(MediaError::Storage(error))
                                    }
                                }
                            }
                        }
                    }
                };
                if media.transfer().is_some() {
                    media_deadline = Some(Instant::now() + Duration::from_secs(60));
                    system
                        .events
                        .send(AppEvent::Gallery(GalleryEvent::Busy(true)))
                        .await;
                } else {
                    media_deadline = None;
                    system
                        .events
                        .send(AppEvent::Gallery(
                            match committed.filter(|_| result.is_ok()) {
                                Some(id) => GalleryEvent::Committed(media.catalog(), id),
                                None => GalleryEvent::Ready(media.catalog()),
                            },
                        ))
                        .await;
                }
                system.media_reply.signal(MediaReply {
                    id: request.id,
                    result,
                });
            }
        }
    }
}

async fn save_settings(
    system: &System,
    flash: &mut impl SlotStorage,
    settings: Settings,
    id: Option<ConfigurationId>,
) {
    system.events.send(AppEvent::StorageBusy(true)).await;
    let event = match e1004_bsp::storage::save(flash, settings) {
        Ok(()) => match id {
            Some(id) => AppEvent::ConfigurationSaved { id, settings },
            None => AppEvent::Configure(settings),
        },
        Err(_) => AppEvent::SettingsFailed,
    };
    system.events.send(event).await;
    system.events.send(AppEvent::StorageBusy(false)).await;
}

pub async fn measurements(
    system: &System,
    mut io: impl crate::measurements::MeasurementIo,
    mut publish: impl FnMut(Sample<u16>),
) {
    loop {
        system.measure.wait().await;
        let result = crate::measurements::measure(&mut io, system.rtc_set.try_take()).await;
        if matches!(result.rtc_write, Some(Err(_))) {
            system.record(LogRecord::Event("rtc_set_failed"));
        }
        publish(result.extension);
        system.extension.lock(|sample| sample.set(result.extension));
        if let Sample::Fresh { value, .. } = result.telemetry.battery {
            system.battery.signal(value.estimated_percent());
        }
        system
            .events
            .send(AppEvent::Telemetry(result.telemetry))
            .await;
    }
}
