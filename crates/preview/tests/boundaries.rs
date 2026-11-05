use device_core::{geometry::Rotation, telemetry::*};
use device_preview::{rgba, MeasurementInputs, Preview};

#[test]
fn recording_and_replay_limits_have_exact_boundaries_and_atomic_failure() {
    let mut preview = Preview::default();
    for _ in 0..4096 {
        preview.command("complete").unwrap();
    }
    let before = (*preview.app(), preview.shown(), preview.recording());
    assert!(preview.command("next").is_err());
    assert!(preview
        .apply_measurements(&MeasurementInputs::default())
        .is_err());
    assert_eq!(
        (*preview.app(), preview.shown(), preview.recording()),
        before
    );
    preview.replay(&"\n".repeat(65536)).unwrap();
    let before = (*preview.app(), preview.shown(), preview.recording());
    assert!(preview.replay(&"\n".repeat(65537)).is_err());
    assert_eq!(
        (*preview.app(), preview.shown(), preview.recording()),
        before
    );
    let values = "2026-11-05T19:30:00 23.45 56.78 3900 unknown absent off off";
    let prefix = "measurements ";
    let command = format!(
        "{prefix}{}{values}",
        " ".repeat(256 - prefix.len() - values.len())
    );
    assert_eq!(command.len(), 256);
    preview.command(&command).unwrap();
    let before = (*preview.app(), preview.shown(), preview.recording());
    assert!(preview.command(&format!("{command} ")).is_err());
    assert_eq!(
        (*preview.app(), preview.shown(), preview.recording()),
        before
    );
}

#[test]
fn every_packed_byte_maps_to_the_documented_opaque_color_channels() {
    let expected = |nibble| match nibble {
        0 => [0, 0, 0, 255],
        1 => [255, 255, 255, 255],
        2 => [255, 255, 0, 255],
        3 => [255, 0, 0, 255],
        5 => [0, 0, 255, 255],
        6 => [0, 255, 0, 255],
        _ => [255, 255, 255, 255],
    };
    for byte in 0u8..=255 {
        let mut pixels = expected(byte / 16).to_vec();
        pixels.extend_from_slice(&expected(byte % 16));
        assert_eq!(rgba(&[byte]), pixels, "byte={byte}");
    }
    assert!(rgba(&[]).is_empty());
}

#[test]
fn every_accepted_recording_fits_the_replay_budget() {
    let mut preview = Preview::default();
    for _ in 0..4096 {
        if preview
            .apply_measurements(&MeasurementInputs::default())
            .is_err()
        {
            break;
        }
    }
    let log = preview.recording();
    assert!(
        log.len() <= 65536,
        "an accepted recording must be replayable"
    );
    let mut replay = Preview::default();
    replay.replay(&log).unwrap();
    assert_eq!(replay.app(), preview.app());
    assert_eq!(replay.shown(), preview.shown());
}

#[test]
fn rotations_are_display_requests_and_late_success_cannot_revive_a_cancelled_frame() {
    for (value, rotation) in [
        (0, Rotation::Portrait),
        (1, Rotation::Landscape),
        (2, Rotation::InvertedPortrait),
        (3, Rotation::InvertedLandscape),
    ] {
        let mut preview = Preview::default();
        preview.command("wait").unwrap();
        preview.command(&format!("rotation {value}")).unwrap();
        let active = preview.app().sequence;
        preview.command("tick 120000").unwrap();
        preview.event(device_core::app::AppEvent::DisplayFinished {
            id: active,
            result: Ok(()),
        });
        assert_eq!(preview.shown().1, Rotation::Portrait);
        preview.command("immediate").unwrap();
        assert_eq!(preview.shown().1, Rotation::Portrait);
        preview.command("refresh").unwrap();
        assert_eq!(preview.shown().1, rotation);
    }
}

#[test]
fn typed_configuration_and_command_configuration_have_identical_orientation_and_pixels() {
    use device_core::{app::AppEvent, settings::Settings};
    for rotation in 0..4 {
        let mut typed = Preview::default();
        typed.event(AppEvent::Configure(Settings {
            rotation,
            ..Settings::default()
        }));
        let mut command = Preview::default();
        command.command(&format!("rotation {rotation}")).unwrap();
        assert_eq!(typed.shown().1, typed.app().rotation);
        assert_eq!(typed.app(), command.app());
        assert!(typed.frame() == command.frame(), "rotation={rotation}");
    }
}

#[test]
fn all_measurement_and_radio_states_have_meaningful_text() {
    let mut preview = Preview::default();
    for (command, expected) in [
        ("sd ready", "ready"),
        ("sd fault", "fault"),
        ("sd absent", "absent"),
        ("wifi connected", "connected"),
        ("ble disconnected", "disconnected"),
        ("charging off", "not charging"),
        ("charging charging", "charging"),
    ] {
        preview.command(command).unwrap();
        assert!(preview.text().contains(expected));
    }
    for fault in [
        Fault::SensorCrc,
        Fault::Bus,
        Fault::PanelTimeout,
        Fault::Storage,
        Fault::InvalidRtc,
        Fault::InvalidInput,
    ] {
        preview.event(device_core::app::AppEvent::Telemetry(TelemetrySnapshot {
            temperature: Sample::Fault(fault),
            ..TelemetrySnapshot::default()
        }));
        assert!(!preview.text().contains("Fault("));
    }
}

#[test]
fn every_direct_control_has_observable_success_and_display_feedback() {
    let mut preview = Preview::default();
    assert!(preview.display_status().contains("Display ready"));
    preview.command("clock 2000-02-29T23:59:59").unwrap();
    assert!(matches!(
        preview.app().telemetry.clock,
        Sample::Fresh {
            value: DateTime {
                year: 2000,
                month: 2,
                day: 29,
                ..
            },
            ..
        }
    ));
    for command in ["wifi connected", "ble connected", "wifi off", "ble off"] {
        preview.command(command).unwrap();
    }
    assert_eq!(
        (
            preview.app().diagnostics.wifi,
            preview.app().diagnostics.ble
        ),
        (RadioStatus::Off, RadioStatus::Off)
    );
    for (command, expected) in [
        ("charging charging", Charging::Charging),
        ("charging off", Charging::NotCharging),
        ("charging unknown", Charging::Unknown),
    ] {
        preview.command(command).unwrap();
        assert_eq!(preview.app().telemetry.charging, expected);
    }
    preview.command("wait").unwrap();
    assert!(preview.display_status().contains("held"));
    preview.command("next").unwrap();
    assert!(preview.display_status().contains("Complete update"));
    preview.command("sleep").unwrap();
    assert!(preview.app().sleep_requested);
    preview.command("next").unwrap();
    assert!(preview.display_status().contains("queued"));
    preview.command("timeout").unwrap();
    assert!(preview.display_status().contains("Refresh to retry"));
}
