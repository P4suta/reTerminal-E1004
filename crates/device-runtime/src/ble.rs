use crate::{
    measurements::IoFailure,
    packet_pool::OwnedPacketPool,
    state::{LogRecord, System, BLE_ACTIVE},
};
use device_core::telemetry::{RadioStatus, Sample};
use embassy_futures::select::{select, Either};
use trouble_host::prelude::*;

const ADVERTISING_BYTES: usize = 31;
const BATTERY_SERVICE_UUID: [u8; 2] = [0x0f, 0x18];
const ADVERTISING_FLAGS: u8 = 0x06;
const ADDRESS_TYPE_BYTE: usize = 5;
const STATIC_RANDOM_ADDRESS_BITS: u8 = 0xc0;

#[gatt_server(packet_type = OwnedPacketPool)]
pub struct Server {
    pub battery: BatteryService,
}
#[gatt_service(uuid = service::BATTERY)]
pub struct BatteryService {
    #[characteristic(uuid = characteristic::BATTERY_LEVEL, read, notify)]
    pub level: u8,
}
pub trait BleFactory {
    type Controller<'a>: trouble_host::Controller
    where
        Self: 'a;
    fn start(&mut self) -> Result<(Self::Controller<'_>, [u8; 6]), IoFailure>;
}
pub fn static_address(mut bytes: [u8; 6]) -> [u8; 6] {
    bytes[ADDRESS_TYPE_BYTE] |= STATIC_RANDOM_ADDRESS_BITS;
    bytes
}
async fn setting(system: &System) -> bool {
    let enabled = system.ble_enabled.wait().await;
    system.radio_changed.signal(());
    enabled
}

pub async fn ble_loop(system: &System, mut factory: impl BleFactory) {
    loop {
        while !setting(system).await {}
        let level = match system.snapshot().map(|app| app.telemetry.battery) {
            Some(Sample::Fresh { value, .. } | Sample::Stale { value, .. }) => {
                value.estimated_percent()
            }
            _ => match select(system.battery.wait(), async {
                while setting(system).await {}
            })
            .await
            {
                Either::First(level) => level,
                Either::Second(()) => continue,
            },
        };
        system.active(BLE_ACTIVE, true);
        match factory.start() {
            Ok((controller, random)) => {
                active(system, controller, static_address(random), level).await
            }
            Err(_) => system.record(LogRecord::Event("ble_initialization_failed")),
        }
        system.active(BLE_ACTIVE, false);
        system.radio_status(None, Some(RadioStatus::Off)).await;
    }
}
pub async fn active<C: trouble_host::Controller>(
    system: &System,
    controller: C,
    random: [u8; 6],
    level: u8,
) {
    let mut resources: HostResources<OwnedPacketPool, 1, 2> = HostResources::new();
    let stack = trouble_host::new(controller, &mut resources)
        .set_random_address(Address::random(random))
        .build();
    let mut runner = stack.runner();
    let mut peripheral = stack.peripheral();
    let Ok(server) = Server::new_with_config(GapConfig::Peripheral(PeripheralConfig {
        name: "reTerminal E1004",
        appearance: &appearance::power_device::GENERIC_POWER_DEVICE,
    })) else {
        return;
    };
    if server.set(&server.battery.level, &level).is_err() {
        system.record(LogRecord::Event("ble_battery_initialization_failed"));
        return;
    }
    system
        .radio_status(None, Some(RadioStatus::Disconnected))
        .await;
    let radio = async {
        loop {
            let mut data = [0u8; ADVERTISING_BYTES];
            let Ok(len) = AdStructure::encode_slice(
                &[
                    AdStructure::Flags(ADVERTISING_FLAGS),
                    AdStructure::CompleteServiceUuids16(&[BATTERY_SERVICE_UUID]),
                    AdStructure::CompleteLocalName(b"E1004"),
                ],
                &mut data,
            ) else {
                return;
            };
            let advertising = peripheral
                .advertise(
                    &Default::default(),
                    Advertisement::ConnectableScannableUndirected {
                        adv_data: &data[..len],
                        scan_data: &[],
                    },
                )
                .await;
            let Ok(advertiser) = advertising else {
                return;
            };
            let Ok(connection) = advertiser.accept().await else {
                continue;
            };
            let Ok(connection) = connection.with_attribute_server(&server) else {
                continue;
            };
            system
                .radio_status(None, Some(RadioStatus::Connected))
                .await;
            loop {
                match select(connection.next(), system.battery.wait()).await {
                    Either::First(GattConnectionEvent::Disconnected { .. }) => break,
                    Either::Second(level) => {
                        if connection.set(&server.battery.level, &level).is_err() {
                            system.record(LogRecord::Event("ble_battery_update_failed"));
                            break;
                        }
                        if server
                            .battery
                            .level
                            .notify(&connection, &level, true)
                            .await
                            .is_err()
                        {
                            system.record(LogRecord::Event("ble_battery_notification_failed"));
                        }
                    }
                    _ => {}
                }
            }
            system
                .radio_status(None, Some(RadioStatus::Disconnected))
                .await;
        }
    };
    let run = async {
        if runner.run().await.is_err() {
            system.record(LogRecord::Event("ble_runner_failed"));
        }
    };
    select(select(run, radio), async { while setting(system).await {} }).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    #[test]
    fn a_setting_waits_for_a_signal_and_acknowledges_its_consumption() {
        let system = System::new(crate::policy::Radios {
            wifi: false,
            ble: true,
        });
        let mut context = Context::from_waker(Waker::noop());
        let mut waiting = core::pin::pin!(setting(&system));
        assert_eq!(waiting.as_mut().poll(&mut context), Poll::Pending);
        system.ble_enabled.signal(true);
        assert_eq!(waiting.as_mut().poll(&mut context), Poll::Ready(true));
        assert!(system.radio_changed.signaled());
        assert!(!system.ble_enabled.signaled());
        system.radio_changed.reset();
        system.ble_enabled.signal(false);
        assert_eq!(
            core::pin::pin!(setting(&system))
                .as_mut()
                .poll(&mut context),
            Poll::Ready(false)
        );
        assert!(system.radio_changed.signaled());
        assert!(!system.ble_enabled.signaled());
    }
}
