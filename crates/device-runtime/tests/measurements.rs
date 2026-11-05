use device_core::telemetry::*;
use device_runtime::measurements::*;
use embassy_time::{Duration, MockDriver};
use std::{
    future::Future,
    task::{Context, Poll, Waker},
};

struct Device {
    enabled: bool,
    trace: Vec<String>,
}
#[derive(Clone, Copy)]
enum Failure {
    None,
    SensorWriteAbsent,
    SensorWriteBus,
    SensorReadAbsent,
    SensorReadBus,
    SensorCrc,
    RtcWrite,
    RtcRead,
    RtcInvalid,
    Charger,
    BatteryPending,
    BatteryBus,
    BatteryOverflow,
    ExtensionPending,
    ExtensionBus,
    ExtensionOverflow,
}
struct FaultDevice {
    inner: Device,
    failure: Failure,
    charger: u8,
}
impl MeasurementIo for FaultDevice {
    fn write(&mut self, address: u8, bytes: &[u8]) -> Result<(), IoFailure> {
        match (address, self.failure) {
            (0x44, Failure::SensorWriteAbsent) => Err(IoFailure::Absent),
            (0x44, Failure::SensorWriteBus) | (0x51, Failure::RtcWrite) => Err(IoFailure::Bus),
            _ => self.inner.write(address, bytes),
        }
    }
    fn read(&mut self, address: u8, bytes: &mut [u8]) -> Result<(), IoFailure> {
        match self.failure {
            Failure::SensorReadAbsent => return Err(IoFailure::Absent),
            Failure::SensorReadBus => return Err(IoFailure::Bus),
            _ => {}
        }
        self.inner.read(address, bytes)?;
        if matches!(self.failure, Failure::SensorCrc) {
            bytes[2] ^= 1;
        }
        Ok(())
    }
    fn write_read(&mut self, address: u8, write: &[u8], read: &mut [u8]) -> Result<(), IoFailure> {
        if matches!(
            (address, self.failure),
            (0x51, Failure::RtcRead) | (0x6b, Failure::Charger)
        ) {
            return Err(IoFailure::Bus);
        }
        self.inner.write_read(address, write, read)?;
        if address == 0x6b {
            read[0] = self.charger;
        }
        if address == 0x51 && matches!(self.failure, Failure::RtcInvalid) {
            read[0] |= 0x80;
        }
        Ok(())
    }
    fn battery_enable(&mut self, enabled: bool) {
        self.inner.battery_enable(enabled);
    }
    fn battery_mv(&mut self) -> nb::Result<u16, IoFailure> {
        match self.failure {
            Failure::BatteryPending => Err(nb::Error::WouldBlock),
            Failure::BatteryBus => Err(nb::Error::Other(IoFailure::Bus)),
            Failure::BatteryOverflow => Ok(u16::MAX),
            _ => self.inner.battery_mv(),
        }
    }
    fn extension_mv(&mut self) -> nb::Result<u16, IoFailure> {
        match self.failure {
            Failure::ExtensionPending => Err(nb::Error::WouldBlock),
            Failure::ExtensionBus => Err(nb::Error::Other(IoFailure::Bus)),
            Failure::ExtensionOverflow => Ok(u16::MAX),
            _ => self.inner.extension_mv(),
        }
    }
}
impl MeasurementIo for Device {
    fn write(&mut self, address: u8, bytes: &[u8]) -> Result<(), IoFailure> {
        self.trace.push(format!("write {address:02x} {bytes:02x?}"));
        Ok(())
    }
    fn read(&mut self, address: u8, bytes: &mut [u8]) -> Result<(), IoFailure> {
        assert_eq!(address, 0x44);
        let crc = e1004_bsp::sensors::sensor_crc([0x80, 0x00]);
        bytes.copy_from_slice(&[0x80, 0x00, crc, 0x80, 0x00, crc]);
        Ok(())
    }
    fn write_read(&mut self, address: u8, write: &[u8], read: &mut [u8]) -> Result<(), IoFailure> {
        match address {
            0x51 => {
                assert_eq!(write, [2]);
                read.copy_from_slice(&[0, 0x30, 0x19, 6, 5, 0x11, 0x26]);
            }
            0x6b => {
                assert_eq!(write, [8]);
                read.copy_from_slice(&[0x10]);
            }
            _ => panic!("unexpected I2C address"),
        }
        Ok(())
    }
    fn battery_enable(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.trace.push(format!("divider {enabled}"));
    }
    fn battery_mv(&mut self) -> nb::Result<u16, IoFailure> {
        assert!(self.enabled);
        Ok(1950)
    }
    fn extension_mv(&mut self) -> nb::Result<u16, IoFailure> {
        assert!(!self.enabled);
        Ok(1234)
    }
}

fn run<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    for _ in 0..1000 {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
        MockDriver::get().advance(Duration::from_millis(1));
    }
    panic!("measurement exceeded the virtual-time budget");
}

#[test]
fn production_measurement_reads_the_documented_registers_and_turns_the_divider_off() {
    MockDriver::get().reset();
    let mut device = Device {
        enabled: false,
        trace: Vec::new(),
    };
    let result = run(measure(&mut device, None));
    assert!(
        matches!(result.telemetry.temperature, Sample::Fresh { value, .. } if value.get() == 4250)
    );
    assert!(
        matches!(result.telemetry.humidity, Sample::Fresh { value, .. } if value.get() == 5650)
    );
    assert!(matches!(result.telemetry.battery, Sample::Fresh { value, .. } if value.get() == 3900));
    assert_eq!(result.telemetry.charging, Charging::Charging);
    assert!(matches!(
        result.extension,
        Sample::Fresh { value: 1234, .. }
    ));
    assert!(!device.enabled);
    assert_eq!(
        device.trace,
        ["write 44 [fd]", "divider true", "divider false"]
    );
    assert_eq!(result.rtc_write, None);
    let time = DateTime::parse("2026-11-06T19:30:00").unwrap();
    for failure in [
        Failure::None,
        Failure::SensorWriteAbsent,
        Failure::SensorWriteBus,
        Failure::SensorReadAbsent,
        Failure::SensorReadBus,
        Failure::SensorCrc,
        Failure::RtcWrite,
        Failure::RtcRead,
        Failure::RtcInvalid,
        Failure::Charger,
        Failure::BatteryPending,
        Failure::BatteryBus,
        Failure::BatteryOverflow,
        Failure::ExtensionPending,
        Failure::ExtensionBus,
        Failure::ExtensionOverflow,
    ] {
        let mut io = FaultDevice {
            inner: Device {
                enabled: false,
                trace: Vec::new(),
            },
            failure,
            charger: 16,
        };
        let result = run(measure(&mut io, Some(time)));
        assert!(!io.inner.enabled);
        match failure {
            Failure::SensorWriteAbsent | Failure::SensorReadAbsent => {
                assert_eq!(result.telemetry.temperature, Sample::Missing);
                assert_eq!(result.telemetry.humidity, Sample::Missing);
            }
            Failure::SensorWriteBus | Failure::SensorReadBus => {
                assert_eq!(result.telemetry.temperature, Sample::Fault(Fault::Bus));
                assert_eq!(result.telemetry.humidity, Sample::Fault(Fault::Bus));
            }
            Failure::SensorCrc => {
                assert_eq!(
                    result.telemetry.temperature,
                    Sample::Fault(Fault::SensorCrc)
                );
                assert_eq!(result.telemetry.humidity, Sample::Fault(Fault::SensorCrc));
            }
            Failure::RtcWrite => assert_eq!(result.rtc_write, Some(Err(IoFailure::Bus))),
            Failure::RtcRead => assert_eq!(result.telemetry.clock, Sample::Fault(Fault::Bus)),
            Failure::RtcInvalid => {
                assert_eq!(result.telemetry.clock, Sample::Fault(Fault::InvalidRtc))
            }
            Failure::Charger => assert_eq!(result.telemetry.charging, Charging::Unknown),
            Failure::BatteryPending | Failure::BatteryBus | Failure::BatteryOverflow => {
                assert_eq!(result.telemetry.battery, Sample::Fault(Fault::Bus))
            }
            Failure::ExtensionPending | Failure::ExtensionBus | Failure::ExtensionOverflow => {
                assert_eq!(result.extension, Sample::Fault(Fault::Bus))
            }
            Failure::None => {
                assert_eq!(result.rtc_write, Some(Ok(())));
                assert!(io.inner.trace[0].starts_with("write 51 [02"));
            }
        }
    }
    for register in 0u8..=255 {
        let mut io = FaultDevice {
            inner: Device {
                enabled: false,
                trace: Vec::new(),
            },
            failure: Failure::None,
            charger: register,
        };
        let result = run(measure(&mut io, None));
        let state = (register / 8) % 4;
        assert_eq!(
            result.telemetry.charging,
            if state == 1 || state == 2 {
                Charging::Charging
            } else {
                Charging::NotCharging
            }
        );
    }
}
