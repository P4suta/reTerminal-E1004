use embassy_executor::Spawner;
#[cfg(feature = "ble")]
mod ble;
#[cfg(feature = "wifi")]
mod wifi;

pub async fn start(
    _spawner: Spawner,
    #[cfg(feature = "wifi")] wifi: esp_hal::peripherals::WIFI<'static>,
    #[cfg(feature = "ble")] bt: esp_hal::peripherals::BT<'static>,
) {
    #[cfg(feature = "wifi")]
    _spawner.spawn(wifi::task(wifi).unwrap());
    #[cfg(feature = "ble")]
    _spawner.spawn(ble::task(bt).unwrap());
}
