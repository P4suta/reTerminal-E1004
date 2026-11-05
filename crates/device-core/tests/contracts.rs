use device_core::{
    geometry::{
        controller_source, pixel_address, replace_nibble, Color, Rotation, FRAME_BYTES, HEIGHT,
        WIDTH,
    },
    settings::{encode, recover, Settings, SLOT_BYTES},
    telemetry::BatteryMillivolts,
};

#[test]
fn physical_coordinates_and_packing_match_the_panel_contract() {
    assert_eq!(FRAME_BYTES, 960_000);
    assert_eq!(Rotation::Portrait.physical(0, 0), Some((0, 0)));
    assert_eq!(Rotation::Landscape.physical(0, 0), Some((0, 1599)));
    assert_eq!(
        Rotation::InvertedPortrait.physical(0, 0),
        Some((1199, 1599))
    );
    assert_eq!(Rotation::InvertedLandscape.physical(0, 0), Some((1199, 0)));
    for rotation in [
        Rotation::Portrait,
        Rotation::Landscape,
        Rotation::InvertedPortrait,
        Rotation::InvertedLandscape,
    ] {
        let (width, height) = rotation.dimensions();
        assert_eq!(rotation.physical(width, 0), None);
        assert_eq!(rotation.physical(0, height), None);
        let mut visited = vec![false; usize::from(WIDTH) * usize::from(HEIGHT)];
        for y in 0..height {
            for x in 0..width {
                let (px, py) = rotation.physical(x, y).unwrap();
                let index = usize::from(py) * usize::from(WIDTH) + usize::from(px);
                assert!(!visited[index], "rotation aliases pixels");
                visited[index] = true;
            }
        }
        assert!(visited.into_iter().all(|value| value));
    }
    assert_eq!(pixel_address(1199, 1599), Some((959_999, false)));
    assert_eq!(pixel_address(1200, 0), None);
    assert_eq!(pixel_address(0, 1600), None);
    assert_eq!(replace_nibble(0x65, true, Color::White), 0x15);
    assert_eq!(replace_nibble(0x65, false, Color::Red), 0x63);
}

#[test]
fn controller_halves_cover_each_frame_byte_exactly_once() {
    let mut visits = vec![0u8; FRAME_BYTES];
    for right in [false, true] {
        for row in 0..1600 {
            for col in 0..300 {
                visits[controller_source(right, row, col).unwrap()] += 1;
            }
        }
    }
    assert!(visits.into_iter().all(|count| count == 1));
    assert_eq!(controller_source(false, 1, 0), Some(600));
    assert_eq!(controller_source(true, 1, 0), Some(900));
    assert_eq!(controller_source(false, 1600, 0), None);
    assert_eq!(controller_source(true, 0, 300), None);
}

#[test]
fn battery_estimate_is_bounded_monotone_and_has_documented_anchors() {
    assert!(BatteryMillivolts::new(1999).is_none());
    assert!(BatteryMillivolts::new(5001).is_none());
    let mut previous = 0;
    for mv in 2000..=5000 {
        let reading = BatteryMillivolts::new(mv).unwrap();
        assert_eq!(reading.get(), mv);
        let percent = reading.estimated_percent();
        assert!(percent <= 100 && percent >= previous);
        previous = percent;
    }
    for (mv, percent) in [
        (3300, 0),
        (3600, 10),
        (3700, 30),
        (3800, 55),
        (3900, 75),
        (4000, 90),
        (4200, 100),
    ] {
        assert_eq!(
            BatteryMillivolts::new(mv).unwrap().estimated_percent(),
            percent
        );
    }
}

#[test]
fn settings_crc_format_and_recovery_reject_invalid_boundaries() {
    use device_core::settings::{crc32, decode};
    assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    for (seconds, valid) in [
        (0, false),
        (59, false),
        (60, true),
        (86400, true),
        (86401, false),
        (u32::MAX, false),
    ] {
        let settings = Settings {
            refresh_seconds: seconds,
            rotation: 3,
        };
        assert_eq!(settings.valid(), valid);
        assert_eq!(decode(&encode(settings, 9)).is_some(), valid);
    }
    for rotation in 0..=255 {
        assert_eq!(
            Settings {
                rotation,
                refresh_seconds: 900
            }
            .valid(),
            rotation < 4
        );
    }
    let valid = encode(Settings::default(), 7);
    assert_eq!(recover(&[], &valid), Some((Settings::default(), 7, 1)));
    assert_eq!(recover(&valid, &[]), Some((Settings::default(), 7, 0)));
    assert_eq!(recover(&valid, &valid), Some((Settings::default(), 7, 0)));
    assert_eq!(
        recover(
            &valid,
            &encode(Settings::default(), 7u32.wrapping_add(1 << 31))
        ),
        Some((Settings::default(), 7, 0))
    );
    for index in [0usize, 4, 6, 7] {
        let mut unsupported = valid;
        unsupported[index] ^= 1;
        let crc = crc32(&unsupported[..16]);
        unsupported[16..20].copy_from_slice(&crc.to_le_bytes());
        assert_eq!(decode(&unsupported), None);
    }
    assert_eq!(
        recover(&valid, &encode(Settings::default(), 6)),
        Some((Settings::default(), 7, 0))
    );
}

#[test]
fn measurements_and_calendar_preserve_units_and_reject_invalid_dates() {
    use device_core::telemetry::*;
    for t in [-4500, -1, 0, 13000] {
        assert_eq!(Temperature::new(t).unwrap().get(), t);
    }
    assert!(Temperature::new(-4501).is_none());
    assert!(Temperature::new(13001).is_none());
    for h in [0, 1, 10000] {
        assert_eq!(Humidity::new(h).unwrap().get(), h);
    }
    assert!(Humidity::new(10001).is_none());
    let time = DateTime::parse("2026-11-05T19:30:00").unwrap();
    assert_eq!(
        (
            time.year,
            time.month,
            time.day,
            time.hour,
            time.minute,
            time.second
        ),
        (2026, 11, 5, 19, 30, 0)
    );
    for bad in [
        "2026-+1-01T00:00:00",
        "2026-01-+1T00:00:00",
        "2026-01-01T+1:00:00",
        "2026-11-05 19:30:00",
        "1999-12-31T23:59:59",
        "2100-01-01T00:00:00",
        "2026-02-29T00:00:00",
        "2026-00-01T00:00:00",
        "2026-13-01T00:00:00",
        "2026-04-31T00:00:00",
        "2026-01-00T00:00:00",
        "2026-01-01T24:00:00",
        "2026-01-01T00:60:00",
        "2026-01-01T00:00:60",
        "2026/11/05T19:30:00",
        "2026-11-05T19-30:00",
        "2026-11-05T19:30-00",
        "😀26-11-05T19:30:00",
    ] {
        assert!(DateTime::parse(bad).is_none(), "{bad}");
    }
    assert!(DateTime::parse("2026-02-28T00:00:00").is_some());
    for index in [4, 7, 10, 13, 16] {
        let mut malformed = *b"2026-11-05T19:30:00";
        malformed[index] = b'/';
        assert!(DateTime::parse(core::str::from_utf8(&malformed).unwrap()).is_none());
    }
    for valid in [
        "2000-02-29T00:00:00",
        "2099-12-31T23:59:59",
        "2026-04-30T00:00:00",
        "2026-01-31T00:00:00",
    ] {
        assert!(DateTime::parse(valid).is_some());
    }
    for sample in [
        Sample::<u8>::Missing,
        Sample::Unavailable,
        Sample::Fault(Fault::Bus),
    ] {
        assert_eq!(sample.at(1000, 0), sample);
    }
}

#[test]
fn interrupted_or_corrupt_settings_preserve_the_previous_committed_slot() {
    let previous = encode(Settings::default(), 41);
    let changed = Settings {
        refresh_seconds: 1800,
        rotation: 1,
    };
    let current = encode(changed, 42);
    assert_eq!(recover(&previous, &current), Some((changed, 42, 1)));
    for cut in 0..SLOT_BYTES {
        let mut interrupted = [0xff; SLOT_BYTES];
        interrupted[..cut].copy_from_slice(&current[..cut]);
        assert_eq!(
            recover(&previous, &interrupted),
            Some((Settings::default(), 41, 0))
        );
    }
    for index in 0..SLOT_BYTES {
        for bit in 0..8 {
            let mut corrupt = current;
            corrupt[index] ^= 1 << bit;
            assert_eq!(
                recover(&previous, &corrupt),
                Some((Settings::default(), 41, 0))
            );
        }
    }
    assert_eq!(recover(&[], &[0xff; SLOT_BYTES]), None);
    let wrap = encode(changed, 0);
    assert_eq!(
        recover(&encode(Settings::default(), u32::MAX), &wrap),
        Some((changed, 0, 1))
    );
}
