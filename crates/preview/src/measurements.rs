use device_core::telemetry::*;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputError {
    field: &'static str,
    message: &'static str,
}
impl InputError {
    pub const fn new(field: &'static str, message: &'static str) -> Self {
        Self { field, message }
    }
    pub const fn field(self) -> &'static str {
        self.field
    }
    pub const fn message(self) -> &'static str {
        self.message
    }
}
impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message)
    }
}

#[derive(Clone, Copy)]
pub struct MeasurementInputs<'a> {
    pub clock: &'a str,
    pub temperature: &'a str,
    pub humidity: &'a str,
    pub battery: &'a str,
    pub charging: &'a str,
    pub sd: &'a str,
    pub wifi: &'a str,
    pub ble: &'a str,
}
impl Default for MeasurementInputs<'_> {
    fn default() -> Self {
        Self {
            clock: "2026-11-05T19:30:00",
            temperature: "23.45",
            humidity: "56.78",
            battery: "3900",
            charging: "unknown",
            sd: "absent",
            wifi: "off",
            ble: "off",
        }
    }
}
pub(crate) struct Measurements {
    pub telemetry: TelemetrySnapshot,
    pub sd: StorageStatus,
    pub wifi: RadioStatus,
    pub ble: RadioStatus,
}
impl MeasurementInputs<'_> {
    pub(crate) fn validate(self, now: u64) -> Result<Measurements, InputError> {
        let clock = DateTime::parse(self.clock).ok_or(InputError::new(
            "clock",
            "RTC time: enter a valid date from 2000 to 2099 as YYYY-MM-DDTHH:MM:SS.",
        ))?;
        let temperature = parse_hundredths(self.temperature)
            .and_then(|n| i16::try_from(n).ok())
            .and_then(Temperature::new)
            .ok_or(InputError::new(
                "temperature",
                "Temperature: enter -45 to 130 C with at most two decimal places.",
            ))?;
        let humidity = parse_hundredths(self.humidity)
            .and_then(|n| u16::try_from(n).ok())
            .and_then(Humidity::new)
            .ok_or(InputError::new(
                "humidity",
                "Relative humidity: enter 0 to 100 % with at most two decimal places.",
            ))?;
        let battery = self
            .battery
            .parse()
            .ok()
            .and_then(BatteryMillivolts::new)
            .ok_or(InputError::new(
                "battery",
                "Battery voltage: enter a whole number from 2000 to 5000 mV.",
            ))?;
        let charging = match self.charging {
            "unknown" => Charging::Unknown,
            "charging" => Charging::Charging,
            "off" => Charging::NotCharging,
            _ => {
                return Err(InputError::new(
                    "charging",
                    "Charging: choose Unknown, Charging, or Not charging.",
                ))
            }
        };
        let sd = match self.sd {
            "absent" => StorageStatus::Absent,
            "ready" => StorageStatus::Ready,
            "fault" => StorageStatus::Fault,
            _ => {
                return Err(InputError::new(
                    "sd",
                    "SD card: choose Absent, Ready, or Fault.",
                ))
            }
        };
        let wifi = radio(self.wifi).ok_or(InputError::new(
            "wifi",
            "Wi-Fi: choose Off, Disconnected, or Connected.",
        ))?;
        let ble = radio(self.ble).ok_or(InputError::new(
            "ble",
            "Bluetooth Low Energy: choose Off, Disconnected, or Connected.",
        ))?;
        Ok(Measurements {
            telemetry: TelemetrySnapshot {
                clock: Sample::Fresh {
                    value: clock,
                    measured_at_ms: now,
                },
                temperature: Sample::Fresh {
                    value: temperature,
                    measured_at_ms: now,
                },
                humidity: Sample::Fresh {
                    value: humidity,
                    measured_at_ms: now,
                },
                battery: Sample::Fresh {
                    value: battery,
                    measured_at_ms: now,
                },
                charging,
            },
            sd,
            wifi,
            ble,
        })
    }
    pub(crate) fn command(self) -> String {
        format!(
            "measurements {} {} {} {} {} {} {} {}",
            self.clock,
            self.temperature,
            self.humidity,
            self.battery,
            self.charging,
            self.sd,
            self.wifi,
            self.ble
        )
    }
}
fn radio(value: &str) -> Option<RadioStatus> {
    match value {
        "off" => Some(RadioStatus::Off),
        "disconnected" => Some(RadioStatus::Disconnected),
        "connected" => Some(RadioStatus::Connected),
        _ => None,
    }
}
