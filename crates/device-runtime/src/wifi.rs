use crate::{
    measurements::IoFailure,
    state::{LogRecord, System, WifiCommand, WIFI_ACTIVE},
};
use device_core::telemetry::RadioStatus;
use embassy_futures::select::{select, Either};
use embassy_time::Timer;

pub const MAX_NETWORKS: usize = 8;
const SSID_BYTES: usize = 32;
const SCAN_TIMEOUT_SECONDS: u64 = 15;
const CONNECTION_TIMEOUT_SECONDS: u64 = 20;

#[derive(Clone, Debug)]
pub struct Network {
    pub ssid: heapless::String<SSID_BYTES>,
    pub signal_dbm: i8,
}
pub trait WifiSession {
    fn scan(
        &mut self,
    ) -> impl core::future::Future<Output = Result<heapless::Vec<Network, MAX_NETWORKS>, IoFailure>>;
    fn connect(
        &mut self,
        ssid: &str,
        password: &str,
    ) -> impl core::future::Future<Output = Result<(), IoFailure>>;
    fn disconnect(&mut self) -> impl core::future::Future<Output = Result<(), IoFailure>>;
}
pub trait WifiFactory {
    type Session<'a>: WifiSession
    where
        Self: 'a;
    fn start(&mut self) -> Result<Self::Session<'_>, IoFailure>;
}

pub async fn wifi_loop(system: &System, mut factory: impl WifiFactory) {
    loop {
        let first = system.wifi_command.receive().await;
        if matches!(first, WifiCommand::Quiesce) {
            system.radio_changed.signal(());
            continue;
        }
        system.active(WIFI_ACTIVE, true);
        match factory.start() {
            Ok(mut session) => {
                let mut command = first;
                let mut connected = false;
                loop {
                    match command {
                        WifiCommand::Scan => {
                            match select(Timer::after_secs(SCAN_TIMEOUT_SECONDS), session.scan())
                                .await
                            {
                                Either::Second(Ok(networks)) => {
                                    system.record(LogRecord::Scan(networks.len() as u8));
                                    for network in networks {
                                        system.record(LogRecord::Network {
                                            ssid: network.ssid,
                                            signal_dbm: network.signal_dbm,
                                        });
                                    }
                                }
                                _ => system.record(LogRecord::Event("wifi_scan_failed")),
                            }
                        }
                        WifiCommand::Connect { ssid, mut password } => {
                            connected = matches!(
                                select(
                                    Timer::after_secs(CONNECTION_TIMEOUT_SECONDS),
                                    session.connect(&ssid, &password)
                                )
                                .await,
                                Either::Second(Ok(()))
                            );
                            password.clear();
                            if !connected {
                                system.record(LogRecord::Event("wifi_connect_failed"));
                            }
                        }
                        WifiCommand::Disconnect => {
                            if !matches!(
                                select(
                                    Timer::after_secs(CONNECTION_TIMEOUT_SECONDS),
                                    session.disconnect()
                                )
                                .await,
                                Either::Second(Ok(()))
                            ) {
                                system.record(LogRecord::Event("wifi_disconnect_failed"));
                            }
                            connected = false;
                        }
                        WifiCommand::Quiesce => break,
                    }
                    system
                        .radio_status(
                            Some(if connected {
                                RadioStatus::Connected
                            } else {
                                RadioStatus::Disconnected
                            }),
                            None,
                        )
                        .await;
                    command = system.wifi_command.receive().await;
                }
            }
            Err(_) => {
                system.record(LogRecord::Event("wifi_initialization_failed"));
            }
        }
        system.active(WIFI_ACTIVE, false);
        system.radio_status(Some(RadioStatus::Off), None).await;
    }
}
