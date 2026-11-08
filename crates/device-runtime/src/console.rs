use crate::{
    measurements::IoFailure,
    state::{BusRequest, LogRecord, System, WifiCommand},
};
use core::{fmt::Write, sync::atomic::Ordering};
use device_core::{
    app::AppEvent,
    settings::{Settings, MAX_REFRESH_SECONDS, MIN_REFRESH_SECONDS},
    telemetry::*,
};
use e1004_bsp::console::{parse, Command, LineInput, LineResult, ParseError};
use embassy_futures::select::{select, Either};
use embassy_time::{Instant, Timer};

const CONSOLE_POLL_MS: u64 = 10;
const COMMAND_WAIT_MS: u64 = 100;
const SERIAL_WAIT_MS: u64 = 1000;
const BUZZER_MS: u64 = 50;
const OUTPUT_BYTES: usize = 1024;
const READ_BATCH: usize = 256;

pub trait ConsoleIo {
    fn read(&mut self) -> nb::Result<u8, IoFailure>;
    fn write(&mut self, bytes: &[u8]) -> nb::Result<usize, IoFailure>;
    fn flush(&mut self) -> nb::Result<(), IoFailure>;
    fn quiet(&mut self);
    fn diagnose(&mut self, enabled: bool);
}

async fn emit(io: &mut impl ConsoleIo, arguments: core::fmt::Arguments<'_>) -> bool {
    let mut text = heapless::String::<OUTPUT_BYTES>::new();
    if text.write_fmt(arguments).is_err() {
        return false;
    }
    let mut remaining = text.as_bytes();
    let deadline = Instant::now() + embassy_time::Duration::from_millis(SERIAL_WAIT_MS);
    while !remaining.is_empty() {
        if Instant::now() >= deadline {
            return false;
        }
        match io.write(remaining) {
            Ok(written) if written > 0 && written <= remaining.len() => {
                remaining = &remaining[written..]
            }
            Err(nb::Error::WouldBlock) => Timer::after_millis(1).await,
            Ok(_) | Err(nb::Error::Other(_)) => return false,
        }
    }
    true
}

async fn bus_command(system: &System, request: BusRequest) -> bool {
    matches!(
        select(
            Timer::after_millis(COMMAND_WAIT_MS),
            system.bus.send(request)
        )
        .await,
        Either::Second(())
    )
}

async fn configure(system: &System, settings: Settings) -> bool {
    let Some(request) = system.reserve_configuration(settings.configuration()) else {
        return false;
    };
    if !bus_command(
        system,
        BusRequest::Configure {
            id: request.id(),
            settings,
        },
    )
    .await
    {
        return false;
    }
    system.request_configuration(request);
    true
}

pub async fn command(system: &System, io: &mut impl ConsoleIo, line: &str) -> bool {
    if line.len() <= 256 {
        if let Some(envelope) = line.strip_prefix("control ") {
            let Some((id, body)) = envelope.split_once(' ') else {
                return false;
            };
            let Ok(id) = id.parse::<u32>() else {
                return false;
            };
            let result = dispatch(system, io, parse(body)).await;
            return emit(
                io,
                format_args!("reply {id} {}\r\n", if result { "ok" } else { "error" }),
            )
            .await
                && result;
        }
    }
    dispatch(system, io, parse(line)).await
}

async fn dispatch(
    system: &System,
    io: &mut impl ConsoleIo,
    parsed: Result<Command<'_>, ParseError>,
) -> bool {
    match parsed {
        Ok(Command::Help) => {
            return emit(
                io,
                format_args!(
                    concat!(
                        "event=help\r\n",
                        "status | diagnose | refresh | sleep\r\n",
                        "interval SECONDS ({} to {} seconds)\r\n",
                        "rtc YYYY-MM-DDTHH:MM:SS (2000 to 2099)\r\n",
                        "resume last | resume status\r\n",
                        "screen status|calendar|checklist|notes|diagnostics|drawing|gallery\r\n",
                        "task set N TEXT | task done N | task open N | task clear N\r\n",
                        "task toggle N (N: 1 to 6; text: 1 to 48 printable ASCII characters)\r\n",
                        "note set TEXT | note clear (up to 160 printable ASCII characters)\r\n",
                        "rotation 0|1|2|3 | hold previous|refresh|next\r\n",
                        "calendar today|YYYY-MM\r\n",
                        "wifi scan | wifi disconnect | wifi connect SSID<TAB>PASSWORD\r\n",
                        "ble on | ble off | awake 0..3600\r\n",
                        "gallery select 1..8 | gallery next | gallery previous\r\n",
                        "gallery slideshow off|60..86400 | gallery fit 0..3 | gallery anchor 0..8\r\n",
                        "image begin ID SLOT ROTATION CRC32 | image data ID OFFSET HEX\r\n",
                        "image commit ID | image abort ID | image delete SLOT | image import SLOT\r\n",
                        "Wi-Fi and BLE commands require their firmware features.\r\n"
                    ),
                    MIN_REFRESH_SECONDS, MAX_REFRESH_SECONDS
                ),
            )
            .await;
        }
        Ok(Command::Refresh) => {
            system
                .events
                .send(AppEvent::Hold(device_core::app::Button::Refresh))
                .await
        }
        Ok(Command::Device(command)) => match command.into_event() {
            Ok(event) => system.events.send(event).await,
            Err(rotation) => {
                let settings = system.configuration_settings();
                return configure(
                    system,
                    Settings {
                        rotation: rotation as u8,
                        ..settings
                    },
                )
                .await;
            }
        },
        Ok(Command::Sleep) => system.events.send(AppEvent::RequestSleep).await,
        Ok(Command::Media(command)) => {
            use device_core::media::GalleryEvent;
            let Some(request) = system.reserve_media(command) else {
                return false;
            };
            if matches!(
                command,
                e1004_bsp::media::MediaCommand::Begin { .. }
                    | e1004_bsp::media::MediaCommand::Delete(_)
                    | e1004_bsp::media::MediaCommand::Import(_)
            ) {
                system.events.send(AppEvent::KeepAwake(600)).await;
                system
                    .events
                    .send(AppEvent::Gallery(GalleryEvent::Busy(true)))
                    .await;
            }
            system.media_reply.reset();
            if !bus_command(system, BusRequest::Media(request)).await {
                system
                    .events
                    .send(AppEvent::Gallery(GalleryEvent::Busy(false)))
                    .await;
                return false;
            }
            let response = async {
                loop {
                    let reply = system.media_reply.wait().await;
                    if reply.id == request.id {
                        return reply.result;
                    }
                }
            };
            return match select(Timer::after_secs(180), response).await {
                Either::Second(Ok(received)) => {
                    emit(
                        io,
                        format_args!("event=image_progress received={received}\r\n"),
                    )
                    .await
                }
                Either::Second(Err(error)) => {
                    emit(
                        io,
                        format_args!("event=image_rejected reason={error:?}\r\n"),
                    )
                    .await;
                    false
                }
                Either::First(()) => false,
            };
        }
        Ok(Command::Diagnose) => {
            let extension = system.extension.lock(core::cell::Cell::get).at(
                Instant::now().as_millis(),
                device_core::app::REFRESH_INTERVAL_MS,
            );
            if !emit(
                io,
                format_args!("event=extension_adc millivolts={extension:?}\r\n"),
            )
            .await
            {
                return false;
            }
            if !bus_command(system, BusRequest::Diagnose).await {
                return false;
            }
            io.diagnose(true);
            Timer::after_millis(BUZZER_MS).await;
            io.diagnose(false);
        }
        Ok(Command::SetInterval(refresh_seconds)) => {
            let settings = system.configuration_settings();
            return configure(
                system,
                Settings {
                    refresh_seconds,
                    ..settings
                },
            )
            .await;
        }
        Ok(Command::Resume(resume)) => {
            let settings = Settings {
                screen: system.snapshot().unwrap_or_default().screen,
                ..system.configuration_settings()
            };
            return configure(
                system,
                Settings {
                    resume,
                    screen: Settings { resume, ..settings }.wake_screen(),
                    ..settings
                },
            )
            .await;
        }
        Ok(Command::SetRtc(text)) => match DateTime::parse(text) {
            Some(time) => {
                system.rtc_set.signal(time);
                system.events.send(AppEvent::MeasurementBusy(true)).await;
                system.measure.signal(());
            }
            None => return false,
        },
        Ok(Command::Status) => {
            if let Some(app) = system.snapshot() {
                if !emit(
                    io,
                    format_args!(
                        "event=gallery selected={} count={} slideshow_s={} busy={}\r\n",
                        app.gallery.settings().selected.number(),
                        app.gallery.catalog().len(),
                        app.gallery.settings().slideshow.map_or(0, |v| v.seconds()),
                        app.gallery.busy()
                    ),
                )
                .await
                {
                    return false;
                }
                for (index, image) in app.gallery.catalog().entries().iter().enumerate() {
                    if let Some(image) = image {
                        if !emit(
                            io,
                            format_args!(
                                "event=image number={} generation={} rotation={} checksum={}\r\n",
                                index + 1,
                                image.generation,
                                image.rotation as u8,
                                image.checksum
                            ),
                        )
                        .await
                        {
                            return false;
                        }
                    }
                }
                if let Some(shown) = system.presented() {
                    if !emit(
                        io,
                        format_args!(
                            "event=shown screen={:?} image={} generation={}\r\n",
                            shown.screen,
                            shown.gallery.view().image.map_or(0, |(id, _)| id.number()),
                            shown
                                .gallery
                                .view()
                                .image
                                .map_or(0, |(_, value)| value.generation)
                        ),
                    )
                    .await
                    {
                        return false;
                    }
                }
                if !emit(io,format_args!("event=status screen={:?} resume={:?} saved_screen={:?} interval_s={} refresh={:?} diagnostics={:?} lost_logs={}\r\n",app.screen,app.resume,app.persisted_settings.screen,app.refresh_seconds,app.refresh,app.diagnostics,system.lost_logs.load(Ordering::Relaxed))).await { return false; }
                if !emit(
                    io,
                    format_args!(
                        "event=daily selected_task={} note={:?} saved={}\r\n",
                        app.selected_task.number(),
                        app.daily.note().as_str(),
                        app.daily == app.persisted_settings.daily
                    ),
                )
                .await
                {
                    return false;
                }
                for (index, task) in app.daily.tasks().iter().enumerate() {
                    if !emit(
                        io,
                        format_args!(
                            "event=task number={} done={} text={:?}\r\n",
                            index + 1,
                            task.done,
                            task.text.as_str()
                        ),
                    )
                    .await
                    {
                        return false;
                    }
                }
                return emit(
                    io,
                    format_args!("event=telemetry value={:?}\r\n", app.telemetry),
                )
                .await;
            }
        }
        Ok(Command::WifiScan | Command::WifiDisconnect | Command::WifiConnect { .. })
            if !system.capabilities.wifi =>
        {
            return emit(
                io,
                format_args!("event=feature_unavailable feature=wifi\r\n"),
            )
            .await;
        }
        Ok(Command::Ble(_)) if !system.capabilities.ble => {
            return emit(
                io,
                format_args!("event=feature_unavailable feature=ble\r\n"),
            )
            .await;
        }
        Ok(Command::WifiScan) => {
            return matches!(
                select(
                    Timer::after_millis(COMMAND_WAIT_MS),
                    system.wifi_command.send(WifiCommand::Scan)
                )
                .await,
                Either::Second(())
            )
        }
        Ok(Command::WifiDisconnect) => {
            return matches!(
                select(
                    Timer::after_millis(COMMAND_WAIT_MS),
                    system.wifi_command.send(WifiCommand::Disconnect)
                )
                .await,
                Either::Second(())
            )
        }
        Ok(Command::WifiConnect { ssid, password }) => {
            let (Ok(ssid), Ok(password)) = (ssid.try_into(), password.try_into()) else {
                return false;
            };
            return matches!(
                select(
                    Timer::after_millis(COMMAND_WAIT_MS),
                    system
                        .wifi_command
                        .send(WifiCommand::Connect { ssid, password })
                )
                .await,
                Either::Second(())
            );
        }
        Ok(Command::Ble(enabled)) => system.ble_enabled.signal(enabled),
        Err(error) => {
            if !emit(
                io,
                format_args!(
                    "event=command_rejected kind={error:?} message={:?}\r\n",
                    error.message()
                ),
            )
            .await
            {
                system.lost_logs.fetch_add(1, Ordering::Relaxed);
            }
            return false;
        }
    }
    true
}

pub async fn console_loop(system: &System, mut io: impl ConsoleIo, version: &str) {
    let mut lines = LineInput::default();
    let mut quiet = false;
    if !emit(
        &mut io,
        format_args!("event=console_ready version={version} hint=help\r\n"),
    )
    .await
    {
        system.lost_logs.fetch_add(1, Ordering::Relaxed);
    }
    loop {
        let sleep = system.output_sleep.try_take();
        if sleep == Some(false) {
            quiet = false;
        }
        if !quiet || sleep == Some(true) {
            while let Ok(log) = system.logs.try_receive() {
                let ok = match log {
                    LogRecord::Fault(fault) => {
                        emit(
                            &mut io,
                            format_args!(
                                "event=fault uptime_ms={} fault={fault:?}\r\n",
                                Instant::now().as_millis()
                            ),
                        )
                        .await
                    }
                    LogRecord::Event(event) => {
                        emit(&mut io, format_args!("event={event}\r\n")).await
                    }
                    LogRecord::Scan(found) => {
                        emit(
                            &mut io,
                            format_args!("event=wifi_scan networks={found}\r\n"),
                        )
                        .await
                    }
                    LogRecord::Network { ssid, signal_dbm } => {
                        emit(
                            &mut io,
                            format_args!(
                                "event=wifi_network ssid={ssid:?} signal_dbm={signal_dbm}\r\n"
                            ),
                        )
                        .await
                    }
                };
                if !ok {
                    system.lost_logs.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        if let Some(sleep) = sleep {
            quiet = sleep;
            if quiet {
                io.quiet();
                let flushed = async {
                    loop {
                        match io.flush() {
                            Ok(()) => return true,
                            Err(nb::Error::WouldBlock) => Timer::after_millis(1).await,
                            Err(nb::Error::Other(_)) => return false,
                        }
                    }
                };
                if matches!(
                    select(Timer::after_millis(SERIAL_WAIT_MS), flushed).await,
                    Either::Second(true)
                ) {
                    system.output_quiet.signal(());
                } else {
                    system.record(LogRecord::Event("console_flush_failed"));
                }
            }
        }
        if !quiet {
            for _ in 0..READ_BATCH {
                let byte = match io.read() {
                    Ok(byte) => byte,
                    Err(nb::Error::WouldBlock) => break,
                    Err(nb::Error::Other(_)) => {
                        system.record(LogRecord::Event("console_read_failed"));
                        break;
                    }
                };
                match lines.push(byte) {
                    LineResult::Complete => {
                        match lines.line() {
                            Ok(line) => {
                                if !command(system, &mut io, line).await
                                    && parse(line).is_ok()
                                    && !emit(&mut io, format_args!("event=command_rejected message=\"Command not completed. Run status, then retry.\"\r\n")).await
                                {
                                    system.lost_logs.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                            Err(_) => {
                                if !emit(&mut io, format_args!("event=command_rejected kind=InvalidText message=\"Command must be UTF-8. Enter help for supported commands.\"\r\n")).await {
                                    system.lost_logs.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                        }
                    }
                    LineResult::TooLong => {
                        if !emit(&mut io, format_args!("event=line_too_long message={:?}\r\n", ParseError::TooLong.message())).await {
                            system.lost_logs.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    LineResult::Pending => {}
                }
            }
        }
        Timer::after_millis(CONSOLE_POLL_MS).await;
    }
}
