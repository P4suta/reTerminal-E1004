use device_runtime::{
    buttons::ButtonsIo,
    console::ConsoleIo,
    measurements::{IoFailure, MeasurementIo},
};
use esp_hal::{
    analog::adc::{Adc, AdcCalCurve, AdcConfig, Attenuation},
    gpio::{Input, Output},
    i2c::master::I2c,
    peripherals::{ADC1, GPIO1, GPIO6},
    uart::Uart,
    Blocking,
};

struct HardwareButtons([Input<'static>; 3]);
impl ButtonsIo for HardwareButtons {
    fn is_high(&mut self, index: usize) -> bool {
        self.0[index].is_high()
    }
    fn wake(&mut self, index: usize, enabled: bool) -> Result<(), IoFailure> {
        let button = &mut self.0[index];
        if enabled {
            button
                .apply_wakeup_config(
                    &esp_hal::gpio::WakeupConfig::default().with_low_power_path(true),
                )
                .map_err(|_| IoFailure::Bus)?;
            button.listen(esp_hal::gpio::Event::LowLevel);
        } else {
            button.unlisten();
        }
        Ok(())
    }
}
#[embassy_executor::task]
pub async fn buttons(buttons: [Input<'static>; 3]) {
    device_runtime::buttons::buttons_loop(&crate::SYSTEM, HardwareButtons(buttons)).await;
}

enum AnalogInput {
    Battery,
    Extension,
}
struct HardwareMeasurements<'a, F> {
    i2c: &'a mut I2c<'static, Blocking>,
    enable: &'a mut Output<'static>,
    adc: F,
}
fn i2c_failure(error: esp_hal::i2c::master::Error) -> IoFailure {
    use embedded_hal::i2c::{Error, ErrorKind, NoAcknowledgeSource};
    match error.kind() {
        ErrorKind::NoAcknowledge(NoAcknowledgeSource::Address) => IoFailure::Absent,
        _ => IoFailure::Bus,
    }
}
impl<F: FnMut(AnalogInput) -> nb::Result<u16, IoFailure>> MeasurementIo
    for HardwareMeasurements<'_, F>
{
    fn write(&mut self, address: u8, bytes: &[u8]) -> Result<(), IoFailure> {
        self.i2c.write(address, bytes).map_err(i2c_failure)
    }
    fn read(&mut self, address: u8, bytes: &mut [u8]) -> Result<(), IoFailure> {
        self.i2c.read(address, bytes).map_err(i2c_failure)
    }
    fn write_read(
        &mut self,
        address: u8,
        bytes: &[u8],
        output: &mut [u8],
    ) -> Result<(), IoFailure> {
        self.i2c
            .write_read(address, bytes, output)
            .map_err(i2c_failure)
    }
    fn battery_enable(&mut self, enabled: bool) {
        self.enable.set_level(enabled.into());
    }
    fn battery_mv(&mut self) -> nb::Result<u16, IoFailure> {
        (self.adc)(AnalogInput::Battery)
    }
    fn extension_mv(&mut self) -> nb::Result<u16, IoFailure> {
        (self.adc)(AnalogInput::Extension)
    }
}
#[embassy_executor::task]
pub async fn measurements(
    mut i2c: I2c<'static, Blocking>,
    adc_instance: ADC1<'static>,
    battery_pin: GPIO1<'static>,
    extension_pin: GPIO6<'static>,
    mut battery_enable: Output<'static>,
) {
    let mut config = AdcConfig::new();
    let mut pin =
        config.enable_pin_with_cal::<_, AdcCalCurve<ADC1>>(battery_pin, Attenuation::_11dB);
    let mut extension =
        config.enable_pin_with_cal::<_, AdcCalCurve<ADC1>>(extension_pin, Attenuation::_11dB);
    let mut adc = Adc::new(adc_instance, config);
    let io = HardwareMeasurements {
        i2c: &mut i2c,
        enable: &mut battery_enable,
        adc: |input| match input {
            AnalogInput::Battery => adc
                .read_oneshot(&mut pin)
                .map_err(|error| error.map(|_| IoFailure::Bus)),
            AnalogInput::Extension => adc
                .read_oneshot(&mut extension)
                .map_err(|error| error.map(|_| IoFailure::Bus)),
        },
    };
    device_runtime::tasks::measurements(&crate::SYSTEM, io, e1004_firmware::publish_extension_adc)
        .await;
}

struct HardwareConsole {
    uart: Uart<'static, Blocking>,
    led: Output<'static>,
    buzzer: Output<'static>,
}
impl ConsoleIo for HardwareConsole {
    fn read(&mut self) -> nb::Result<u8, IoFailure> {
        if !self.uart.read_ready() {
            return Err(nb::Error::WouldBlock);
        }
        let mut byte = [0];
        match self.uart.read(&mut byte) {
            Ok(1) => Ok(byte[0]),
            Ok(_) => Err(nb::Error::WouldBlock),
            Err(_) => Err(nb::Error::Other(IoFailure::Bus)),
        }
    }
    fn write(&mut self, bytes: &[u8]) -> nb::Result<usize, IoFailure> {
        self.uart
            .write(bytes)
            .map_err(|_| nb::Error::Other(IoFailure::Bus))
    }
    fn flush(&mut self) -> nb::Result<(), IoFailure> {
        self.uart
            .flush()
            .map_err(|_| nb::Error::Other(IoFailure::Bus))
    }
    fn quiet(&mut self) {
        self.led.set_high();
        self.buzzer.set_low();
    }
    fn diagnose(&mut self, enabled: bool) {
        self.led.set_level((!enabled).into());
        self.buzzer.set_level(enabled.into());
    }
}
#[embassy_executor::task]
pub async fn console(uart: Uart<'static, Blocking>, led: Output<'static>, buzzer: Output<'static>) {
    device_runtime::console::console_loop(
        &crate::SYSTEM,
        HardwareConsole { uart, led, buzzer },
        env!("CARGO_PKG_VERSION"),
    )
    .await;
}
