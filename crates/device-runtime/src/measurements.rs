use device_core::telemetry::*;
use e1004_bsp::sensors::{rtc_decode, rtc_encode, sht40_decode};
use embassy_time::{Duration, Instant, Timer};

const SENSOR_ADDRESS: u8 = 0x44;
const SENSOR_MEASURE_COMMAND: u8 = 0xfd;
const RTC_ADDRESS: u8 = 0x51;
const RTC_SECONDS_REGISTER: u8 = 2;
const CHARGER_ADDRESS: u8 = 0x6b;
const CHARGE_STATUS_REGISTER: u8 = 8;
const CHARGE_STATUS_SHIFT: u32 = 3;
const CHARGE_STATUS_MASK: u8 = 3;
const SETTLE_MS: u64 = 10;
const ADC_TIMEOUT_MS: u64 = 50;
const ADC_POLL_MS: u64 = 1;
const BATTERY_DIVIDER: u16 = 2;
const EXTENSION_MAX_MILLIVOLTS: u16 = 3300;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IoFailure {
    Absent,
    Bus,
}

pub trait MeasurementIo {
    fn write(&mut self, address: u8, bytes: &[u8]) -> Result<(), IoFailure>;
    fn read(&mut self, address: u8, bytes: &mut [u8]) -> Result<(), IoFailure>;
    fn write_read(&mut self, address: u8, write: &[u8], read: &mut [u8]) -> Result<(), IoFailure>;
    fn battery_enable(&mut self, enabled: bool);
    fn battery_mv(&mut self) -> nb::Result<u16, IoFailure>;
    fn extension_mv(&mut self) -> nb::Result<u16, IoFailure>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Measurement {
    pub telemetry: TelemetrySnapshot,
    pub extension: Sample<u16>,
    pub rtc_write: Option<Result<(), IoFailure>>,
}

pub fn charging_status(register: u8) -> Charging {
    match (register >> CHARGE_STATUS_SHIFT) & CHARGE_STATUS_MASK {
        1 | 2 => Charging::Charging,
        _ => Charging::NotCharging,
    }
}

fn sample<T>(value: Result<T, IoFailure>, measured_at_ms: u64) -> Sample<T> {
    match value {
        Ok(value) => Sample::Fresh {
            value,
            measured_at_ms,
        },
        Err(IoFailure::Absent) => Sample::Missing,
        Err(IoFailure::Bus) => Sample::Fault(Fault::Bus),
    }
}

async fn adc(mut read: impl FnMut() -> nb::Result<u16, IoFailure>) -> Result<u16, IoFailure> {
    let deadline = Instant::now() + Duration::from_millis(ADC_TIMEOUT_MS);
    loop {
        if Instant::now() >= deadline {
            return Err(IoFailure::Bus);
        }
        match read() {
            Ok(value) => return Ok(value),
            Err(nb::Error::Other(error)) => return Err(error),
            Err(nb::Error::WouldBlock) => Timer::after_millis(ADC_POLL_MS).await,
        }
    }
}

pub async fn measure(io: &mut impl MeasurementIo, set_rtc: Option<DateTime>) -> Measurement {
    let rtc_write = set_rtc.map(|time| match rtc_encode(time) {
        Ok(bytes) => io.write(RTC_ADDRESS, &bytes),
        Err(_) => Err(IoFailure::Bus),
    });
    let measured_at_ms = Instant::now().as_millis();
    let mut telemetry = TelemetrySnapshot::default();
    let mut sensor = [0u8; 6];
    let sensor_result = match io.write(SENSOR_ADDRESS, &[SENSOR_MEASURE_COMMAND]) {
        Ok(()) => {
            Timer::after_millis(SETTLE_MS).await;
            io.read(SENSOR_ADDRESS, &mut sensor)
        }
        Err(error) => Err(error),
    };
    match sensor_result {
        Ok(()) => match sht40_decode(sensor) {
            Ok((temperature, humidity)) => {
                telemetry.temperature = sample(Ok(temperature), measured_at_ms);
                telemetry.humidity = sample(Ok(humidity), measured_at_ms);
            }
            Err(fault) => {
                telemetry.temperature = Sample::Fault(fault);
                telemetry.humidity = Sample::Fault(fault);
            }
        },
        Err(error) => {
            telemetry.temperature = sample(Err(error), measured_at_ms);
            telemetry.humidity = sample(Err(error), measured_at_ms);
        }
    }
    let mut rtc = [0u8; 7];
    telemetry.clock = match io.write_read(RTC_ADDRESS, &[RTC_SECONDS_REGISTER], &mut rtc) {
        Ok(()) => match rtc_decode(rtc) {
            Ok(value) => sample(Ok(value), measured_at_ms),
            Err(fault) => Sample::Fault(fault),
        },
        Err(error) => sample(Err(error), measured_at_ms),
    };
    let mut charger = [0u8; 1];
    telemetry.charging = if io
        .write_read(CHARGER_ADDRESS, &[CHARGE_STATUS_REGISTER], &mut charger)
        .is_ok()
    {
        charging_status(charger[0])
    } else {
        Charging::Unknown
    };
    io.battery_enable(true);
    Timer::after_millis(SETTLE_MS).await;
    let battery = adc(|| io.battery_mv()).await.and_then(|value| {
        value
            .checked_mul(BATTERY_DIVIDER)
            .and_then(BatteryMillivolts::new)
            .ok_or(IoFailure::Bus)
    });
    io.battery_enable(false);
    telemetry.battery = sample(battery, measured_at_ms);
    let extension = adc(|| io.extension_mv()).await.and_then(|value| {
        (value <= EXTENSION_MAX_MILLIVOLTS)
            .then_some(value)
            .ok_or(IoFailure::Bus)
    });
    Measurement {
        telemetry,
        extension: sample(extension, measured_at_ms),
        rtc_write,
    }
}
