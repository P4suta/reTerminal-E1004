use crate::{input::Debouncer, sensors::*};
use device_core::telemetry::{DateTime, Fault};

#[kani::proof]
#[kani::unwind(2)]
fn flash_addresses_are_bounded_and_sectors_are_disjoint() {
    let slot: usize = kani::any();
    let offset: usize = kani::any();
    let length: usize = kani::any();
    let result = crate::storage::settings_address(slot, offset, length);
    let valid = slot < 2 && offset < 24 && (offset as u128) + (length as u128) <= 24;
    assert_eq!(result.is_ok(), valid);
    if let Ok(address) = result {
        assert_eq!(
            u128::from(address),
            0x9000 + (slot as u128) * 4096 + (offset as u128)
        );
        assert!(address >= 0x9000 && address < 0xb000);
    }
    kani::cover!(result.is_ok() && slot == 1);
}

#[path = "../../../firmware/src/effects.rs"]
mod effects;

#[kani::proof]
#[kani::unwind(2)]
fn console_step_preserves_buffer_and_discard_invariants() {
    let mut input = crate::console::LineInput::proof_state(kani::any(), kani::any(), kani::any());
    kani::assume(input.invariant());
    input.push(kani::any());
    assert!(input.invariant());
    let request: u8 = kani::any();
    let mut pending = Some(request);
    effects::dispatch(&mut pending, Some);
    assert_eq!(pending, Some(request));
    effects::dispatch(&mut pending, |_| None);
    assert_eq!(pending, None);
}

#[kani::proof]
#[kani::unwind(4)]
fn stable_button_never_repeats_a_press() {
    let mut input = Debouncer::default();
    assert!(!input.update(true));
    assert!(!input.update(true));
    assert!(input.update(true));
    let pressed: bool = kani::any();
    assert!(!input.update(pressed));
    assert!(!input.update(pressed));
    assert!(!input.update(pressed));
}

#[kani::proof]
#[kani::unwind(20)]
fn sensor_words_convert_without_overflow() {
    let raw: [u8; 4] = kani::any();
    let bytes = [
        raw[0],
        raw[1],
        sensor_crc([raw[0], raw[1]]),
        raw[2],
        raw[3],
        sensor_crc([raw[2], raw[3]]),
    ];
    let (temperature, humidity) = sht40_decode(bytes).unwrap();
    assert!((-4500..=13000).contains(&temperature.get()));
    assert!(humidity.get() <= 10000);
    kani::cover!(temperature.get() < 0 && humidity.get() == 0);
    kani::cover!(humidity.get() == 10000);
}

#[kani::proof]
#[kani::unwind(3)]
fn calendar_round_trip_preserves_all_valid_fields() {
    let time = DateTime {
        year: kani::any(),
        month: kani::any(),
        day: kani::any(),
        hour: kani::any(),
        minute: kani::any(),
        second: kani::any(),
    };
    kani::assume(time.valid());
    let bytes = rtc_encode(time).unwrap();
    let registers = [
        bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ];
    assert_eq!(rtc_decode(registers), Ok(time));
    let mut invalid = registers;
    invalid[0] |= 0x80;
    assert_eq!(rtc_decode(invalid), Err(Fault::InvalidRtc));
    kani::cover!(time.month == 2 && time.day == 29);
}
