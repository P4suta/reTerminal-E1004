use device_core::{
    settings::{encode, recover, Settings, SLOT_BYTES},
    telemetry::*,
};
use e1004_bsp::{console::*, sensors::*, storage::*};

#[path = "../../../firmware/src/effects.rs"]
mod effects;

#[test]
fn a_full_effect_queue_keeps_the_request_and_the_event_pump_can_continue() {
    let mut pending = Some(7u8);
    let mut polls = 0;
    for _ in 0..1000 {
        effects::dispatch(&mut pending, |request| {
            polls += 1;
            Some(request)
        });
        assert_eq!(pending, Some(7));
    }
    assert_eq!(polls, 1000);
    effects::dispatch(&mut pending, |_| None);
    assert_eq!(pending, None);
    effects::dispatch(&mut pending, |_| {
        panic!("an empty request must not call the queue")
    });
}

#[derive(Clone)]
struct Slots {
    slots: [[u8; SLOT_BYTES]; 2],
    operations: usize,
    fail_at: Option<usize>,
    absent: bool,
}
impl Slots {
    fn operation(&mut self) -> Result<(), StorageError> {
        self.operations += 1;
        if self.absent {
            Err(StorageError::Absent)
        } else if self.fail_at == Some(self.operations) {
            Err(StorageError::Io)
        } else {
            Ok(())
        }
    }
}
impl SlotStorage for Slots {
    fn read(&mut self, slot: usize, bytes: &mut [u8; SLOT_BYTES]) -> Result<(), StorageError> {
        self.operation()?;
        *bytes = self.slots[slot];
        Ok(())
    }
    fn write(&mut self, slot: usize, offset: usize, bytes: &[u8]) -> Result<(), StorageError> {
        self.operation()?;
        self.slots[slot][offset..offset + bytes.len()].copy_from_slice(bytes);
        Ok(())
    }
    fn sync(&mut self) -> Result<(), StorageError> {
        self.operation()
    }
}

#[test]
fn settings_io_failure_at_every_operation_keeps_a_valid_committed_value() {
    let settings = Settings {
        refresh_seconds: 1800,
        rotation: 2,
        ..Settings::default()
    };
    let original = Slots {
        slots: [encode(Settings::default(), 4), [0xff; SLOT_BYTES]],
        operations: 0,
        fail_at: None,
        absent: false,
    };
    let mut success = original.clone();
    save(&mut success, settings).unwrap();
    assert_eq!(
        recover(&success.slots[0], &success.slots[1]),
        Some((settings, 5, 1))
    );
    for failure in 1..=success.operations {
        let mut slots = Slots {
            fail_at: Some(failure),
            ..original.clone()
        };
        assert!(save(&mut slots, settings).is_err());
        let recovered = recover(&slots.slots[0], &slots.slots[1]).unwrap().0;
        assert!(recovered == Settings::default() || recovered == settings);
    }
    let mut absent = Slots {
        absent: true,
        ..original
    };
    assert_eq!(save(&mut absent, settings), Err(StorageError::Absent));
}

#[test]
fn rtc_rejects_voltage_loss_invalid_bcd_and_impossible_dates() {
    let time = DateTime {
        year: 2026,
        month: 11,
        day: 5,
        hour: 19,
        minute: 3,
        second: 42,
    };
    let encoded = rtc_encode(time).unwrap();
    assert_eq!(encoded[0], 2);
    assert_eq!(rtc_decode(encoded[1..].try_into().unwrap()), Ok(time));
    assert_eq!(rtc_decode([0x80, 0, 0, 1, 0, 1, 0]), Err(Fault::InvalidRtc));
    assert!(rtc_decode([0x6a, 0, 0, 1, 0, 1, 0]).is_err());
    assert!(rtc_decode([0, 0, 0, 0x31, 0, 0x02, 0x26]).is_err());
    assert!(rtc_decode([0, 0, 0, 1, 0, 0x81, 0]).is_err());
}

fn crc(bytes: [u8; 2]) -> u8 {
    let mut crc = 0xff;
    for byte in bytes {
        crc ^= byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x31
            } else {
                crc << 1
            };
        }
    }
    crc
}

#[test]
fn sensor_crc_and_full_scale_conversion_are_checked() {
    for (raw, expected) in [(0u16, -4500), (u16::MAX, 13000), (32768, 4250)] {
        let [a, b] = raw.to_be_bytes();
        let [c, d] = 32768u16.to_be_bytes();
        let bytes = [a, b, crc([a, b]), c, d, crc([c, d])];
        let (temperature, humidity) = sht40_decode(bytes).unwrap();
        assert_eq!(temperature.get(), expected);
        assert_eq!(humidity.get(), 5650);
        for i in 0..6 {
            let mut damaged = bytes;
            damaged[i] ^= 1;
            assert_eq!(sht40_decode(damaged), Err(Fault::SensorCrc));
        }
    }
}

#[test]
fn console_accepts_only_bounded_known_commands() {
    assert_eq!(parse("status"), Ok(Command::Status));
    assert_eq!(parse("diagnose"), Ok(Command::Diagnose));
    assert_eq!(parse("interval 900"), Ok(Command::SetInterval(900)));
    assert_eq!(parse("interval 59"), Err(ParseError::Invalid));
    assert_eq!(parse("interval 86401"), Err(ParseError::Invalid));
    assert_eq!(
        parse("wifi connect Office\tsecret phrase"),
        Ok(Command::WifiConnect {
            ssid: "Office",
            password: "secret phrase"
        })
    );
    assert!(parse("wifi connect Office").is_err());
    assert_eq!(parse("ble on"), Ok(Command::Ble(true)));
    assert!(parse("ble maybe").is_err());
    assert_eq!(parse(&"x".repeat(257)), Err(ParseError::TooLong));
    assert_eq!(parse("rtc 2026-02-30T12:00:00"), Err(ParseError::Invalid));
}

#[test]
fn an_oversized_console_line_cannot_execute_its_tail() {
    let mut input = LineInput::default();
    for byte in core::iter::repeat_n(b'x', 257).chain(*b"sleep\n") {
        assert!(!matches!(input.push(byte), LineResult::Complete));
    }
    for byte in b"status" {
        assert_eq!(input.push(*byte), LineResult::Pending);
    }
    assert_eq!(input.push(b'\n'), LineResult::Complete);
    assert_eq!(input.line(), Ok("status"));
    assert_eq!(input.push(b'\r'), LineResult::Pending);
    for byte in [0xff, b'\n'] {
        input.push(byte);
    }
    assert!(input.line().is_err());
}
