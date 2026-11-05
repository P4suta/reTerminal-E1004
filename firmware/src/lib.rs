#![no_std]

use core::cell::Cell;
use device_core::telemetry::Sample;
use esp_hal::peripherals::*;

static ANALOG: embassy_sync::blocking_mutex::Mutex<
    embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
    Cell<Sample<u16>>,
> = embassy_sync::blocking_mutex::Mutex::new(Cell::new(Sample::Unavailable));

pub struct ExtensionAdc;
impl ExtensionAdc {
    pub fn millivolts(&self, now_ms: u64) -> Sample<u16> {
        ANALOG
            .lock(Cell::get)
            .at(now_ms, device_core::app::REFRESH_INTERVAL_MS)
    }
}
pub fn publish_extension_adc(sample: Sample<u16>) {
    ANALOG.lock(|value| value.set(sample));
}

pub struct Extensions {
    pub sda: GPIO39<'static>,
    pub scl: GPIO40<'static>,
    pub rx: GPIO41<'static>,
    pub tx: GPIO42<'static>,
    pub analog: ExtensionAdc,
    pub i2c: I2C1<'static>,
    pub uart: UART1<'static>,
}
