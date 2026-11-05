use crate::{
    measurements::IoFailure,
    state::{BusRequest, LogRecord, System, WifiCommand},
};
use core::{fmt::Write, sync::atomic::Ordering};
use device_core::{app::AppEvent, settings::Settings, telemetry::*};
use e1004_bsp::console::{parse, Command, LineInput, LineResult};
use embassy_futures::select::{select, Either};
use embassy_time::{Instant, Timer};

const CONSOLE_POLL_MS: u64 = 10;
const COMMAND_WAIT_MS: u64 = 100;
const SERIAL_WAIT_MS: u64 = 1000;
const BUZZER_MS: u64 = 50;
const OUTPUT_BYTES: usize = 1024;
const READ_BATCH: usize = 32;

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

pub async fn command(system: &System, io: &mut impl ConsoleIo, line: &str) -> bool {
    match parse(line) {
        Ok(Command::Refresh) => {
            system
                .events
                .send(AppEvent::Button(device_core::app::Button::Refresh))
                .await
        }
        Ok(Command::Sleep) => system.events.send(AppEvent::RequestSleep).await,
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
            let rotation = system.snapshot().map_or(0, |app| app.rotation as u8);
            return bus_command(
                system,
                BusRequest::Save(Settings {
                    refresh_seconds,
                    rotation,
                }),
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
                if !emit(io,format_args!("event=status screen={:?} interval_s={} refresh={:?} diagnostics={:?} lost_logs={}\r\n",app.screen,app.refresh_seconds,app.refresh,app.diagnostics,system.lost_logs.load(Ordering::Relaxed))).await { return false; }
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
        Err(_) => return false,
    }
    true
}

pub async fn console_loop(system: &System, mut io: impl ConsoleIo, version: &str) {
    let mut lines = LineInput::default();
    let mut quiet = false;
    if !emit(
        &mut io,
        format_args!("event=console_ready version={version}\r\n"),
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
                        if let Ok(line) = lines.line() {
                            if !command(system, &mut io, line).await
                                && !emit(&mut io, format_args!("event=command_rejected\r\n")).await
                            {
                                system.lost_logs.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }
                    LineResult::TooLong => {
                        if !emit(&mut io, format_args!("event=line_too_long\r\n")).await {
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
