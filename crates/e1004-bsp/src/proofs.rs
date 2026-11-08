use crate::{input::Debouncer, sensors::*};
use device_core::telemetry::{DateTime, Fault};

#[kani::proof]
#[kani::unwind(8)]
fn resume_policy_decoding_accepts_only_exact_ascii_values() {
    use crate::console::resume_policy;
    use device_core::settings::ResumePolicy;
    let bytes: [u8; 6] = kani::any();
    for byte in bytes {
        kani::assume(byte.is_ascii());
    }
    let length: usize = kani::any();
    kani::assume(length <= bytes.len());
    let input = core::str::from_utf8(&bytes[..length]).unwrap();
    let last =
        length == 4 && bytes[0] == b'l' && bytes[1] == b'a' && bytes[2] == b's' && bytes[3] == b't';
    let status = length == 6
        && bytes[0] == b's'
        && bytes[1] == b't'
        && bytes[2] == b'a'
        && bytes[3] == b't'
        && bytes[4] == b'u'
        && bytes[5] == b's';
    let expected = if last {
        Some(ResumePolicy::Last)
    } else if status {
        Some(ResumePolicy::Status)
    } else {
        None
    };
    assert_eq!(resume_policy(input), expected);
    kani::cover!(last);
    kani::cover!(status);
    kani::cover!(expected.is_none());
}

#[kani::proof]
#[kani::unwind(2)]
fn flash_addresses_are_bounded_and_sectors_are_disjoint() {
    let slot: usize = kani::any();
    let offset: usize = kani::any();
    let length: usize = kani::any();
    let result = crate::storage::settings_address(slot, offset, length);
    let valid = slot < 2
        && offset < device_core::settings::SLOT_BYTES
        && (offset as u128) + (length as u128) <= device_core::settings::SLOT_BYTES as u128;
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
    let length = kani::any();
    let discarding = kani::any();
    let complete = kani::any();
    let mut input = crate::console::LineInput::proof_state(length, discarding, complete);
    assert_eq!(input.proof_components(), (length, discarding, complete));
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
#[kani::unwind(2)]
fn wifi_credential_lengths_match_the_byte_limits() {
    let ssid: usize = kani::any();
    let password: usize = kani::any();
    const SSID_LIMIT: usize = 32;
    const PASSWORD_LIMIT: usize = 63;
    assert_eq!(
        crate::console::wifi_credential_lengths_valid(ssid, password),
        (1..=SSID_LIMIT).contains(&ssid) && password <= PASSWORD_LIMIT
    );
    kani::cover!(ssid == SSID_LIMIT && password == PASSWORD_LIMIT);
    kani::cover!(
        ssid > 0 && password == 0 && crate::console::wifi_credential_lengths_valid(ssid, password)
    );
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
#[kani::unwind(4)]
fn held_button_emits_once_and_never_becomes_a_tap() {
    use crate::input::{Gesture, GestureEvent, HOLD_MS};
    let mut gesture = Gesture::default();
    assert!(gesture.is_idle());
    for now in [0, 20, 40] {
        assert_eq!(gesture.update(true, now), None);
    }
    assert_eq!(gesture.update(true, 40 + HOLD_MS), Some(GestureEvent::Hold));
    assert!(!gesture.is_idle());
    let later: u64 = kani::any();
    kani::assume(later >= 40 + HOLD_MS);
    assert_eq!(gesture.update(true, later), None);
    for now in [later, later, later] {
        assert_eq!(gesture.update(false, now), None);
    }
    assert!(gesture.is_idle());
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
#[kani::proof]
fn image_flash_addresses_stay_in_the_reserved_partition() {
    use crate::media::{media_address, MEDIA_BASE, MEDIA_SLOT_BYTES};
    use device_core::media::PhysicalSlot;
    let index: u8 = kani::any();
    kani::assume(index < 9);
    let slot = PhysicalSlot::new(index).unwrap();
    let offset: u32 = kani::any();
    let length: u32 = kani::any();
    let result = media_address(slot, offset as usize, length as usize);
    assert_eq!(
        result.is_ok(),
        u64::from(offset) + u64::from(length) <= u64::from(MEDIA_SLOT_BYTES)
    );
    if let Ok(address) = result {
        assert!(address >= MEDIA_BASE);
        assert!(
            u64::from(address) + u64::from(length)
                <= u64::from(MEDIA_BASE) + 9 * u64::from(MEDIA_SLOT_BYTES)
        );
    }
    kani::cover!(result.is_ok());
    kani::cover!(result.is_err());
}
