use crate::{policy::*, state::*};
use core::sync::atomic::Ordering;
use device_core::{
    app::*,
    geometry::Rotation,
    settings::{Settings, SLOT_BYTES},
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
    mut hardware: impl BusIo,
    mut flash: impl SlotStorage,
    frame: &mut [u8],
) {
    let mut panel = t133a01::Panel::default();
    let mut panel_sleeping = false;
    let mut resume_attempt = None;
    loop {
        let request = match select(system.bus.receive(), system.sleep_request.wait()).await {
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
                system.stop_panel.reset();
                system.start_display(id, app);
                let result = match Framebuffer::new(frame, app.rotation) {
                    Ok(mut target) => {
                        device_ui::render(&app, &mut target);
                        panel_sleeping = false;
                        panel_update(system, &mut panel, &mut hardware, frame).await
                    }
                    Err(_) => Err(Fault::Bus),
                };
                system.finish_display(id, result);
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
                            save_settings(system, &mut flash, desired).await;
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
                save_settings(system, &mut flash, settings).await;
            }
            BusRequest::Diagnose => {
                system.events.send(AppEvent::StorageBusy(true)).await;
                let status = hardware.sd_diagnose();
                system.events.send(AppEvent::StorageChanged(status)).await;
                system.events.send(AppEvent::StorageBusy(false)).await;
            }
        }
    }
}

async fn save_settings(system: &System, flash: &mut impl SlotStorage, settings: Settings) {
    system.events.send(AppEvent::StorageBusy(true)).await;
    let event = match e1004_bsp::storage::save(flash, settings) {
        Ok(()) => AppEvent::Configure(settings),
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
