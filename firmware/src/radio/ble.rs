use device_runtime::{ble::BleFactory, measurements::IoFailure};
use trouble_host::prelude::ExternalController;
const CONTROLLER_COMMAND_SLOTS: usize = 20;
struct Factory(esp_hal::peripherals::BT<'static>);
impl BleFactory for Factory {
    type Controller<'a> =
        ExternalController<esp_radio::ble::controller::BleConnector<'a>, CONTROLLER_COMMAND_SLOTS>;
    fn start(&mut self) -> Result<(Self::Controller<'_>, [u8; 6]), IoFailure> {
        let connector =
            esp_radio::ble::controller::BleConnector::new(self.0.reborrow(), Default::default())
                .map_err(|_| IoFailure::Bus)?;
        let mut random = [0; 6];
        esp_hal::rng::Rng::new().read(&mut random);
        Ok((ExternalController::new(connector), random))
    }
}
#[embassy_executor::task]
pub async fn task(bt: esp_hal::peripherals::BT<'static>) {
    device_runtime::ble::ble_loop(&crate::SYSTEM, Factory(bt)).await;
}
