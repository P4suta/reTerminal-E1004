use device_core::{app::RefreshState, telemetry::*};
use device_preview::{MeasurementInputs, Preview};

#[test]
fn a_form_submission_validates_all_fields_before_one_display_request() {
    let mut preview = Preview::default();
    let inputs = MeasurementInputs::default();
    preview.apply_measurements(&inputs).unwrap();
    assert_eq!(preview.app().sequence, 1);
    assert_eq!(preview.app().refresh, RefreshState::Idle);
    assert!(
        matches!(preview.app().telemetry.temperature, Sample::Fresh { value, .. } if value.get() == 2345)
    );
    let original = (preview.shown(), *preview.app(), preview.recording());
    for (field, invalid) in [
        (
            "clock",
            MeasurementInputs {
                clock: "2025-02-29T00:00:00",
                ..inputs
            },
        ),
        (
            "temperature",
            MeasurementInputs {
                temperature: "130.01",
                ..inputs
            },
        ),
        (
            "humidity",
            MeasurementInputs {
                humidity: "100.01",
                ..inputs
            },
        ),
        (
            "battery",
            MeasurementInputs {
                battery: "1999",
                ..inputs
            },
        ),
        (
            "charging",
            MeasurementInputs {
                charging: "yes",
                ..inputs
            },
        ),
        (
            "sd",
            MeasurementInputs {
                sd: "maybe",
                ..inputs
            },
        ),
        (
            "wifi",
            MeasurementInputs {
                wifi: "yes",
                ..inputs
            },
        ),
        (
            "ble",
            MeasurementInputs {
                ble: "yes",
                ..inputs
            },
        ),
    ] {
        let error = preview.apply_measurements(&invalid).unwrap_err();
        assert_eq!(error.field(), field);
        assert_eq!(error.message(), error.to_string());
        assert!(!error.to_string().is_empty());
        assert_eq!(
            (preview.shown(), *preview.app(), preview.recording()),
            original
        );
    }
    let recording = preview.recording();
    let mut replay = Preview::default();
    replay.replay(&recording).unwrap();
    assert_eq!(preview.app(), replay.app());
    assert!(preview.frame() == replay.frame());
}

#[test]
fn human_readable_information_preserves_missing_fault_and_stale_values() {
    let mut preview = Preview::default();
    preview
        .apply_measurements(&MeasurementInputs::default())
        .unwrap();
    let text = preview.text();
    for expected in [
        "Device Status",
        "23.45 C",
        "56.78 %",
        "3900 mV",
        "75%",
        "2026-11-05 19:30:00",
        "fresh",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    for (command, expected) in [
        ("temperature missing", "not present"),
        ("temperature fault", "bus error"),
        ("temperature unknown", "not measured"),
    ] {
        preview.command(command).unwrap();
        assert!(preview.text().contains(expected));
    }
    preview.command("humidity 5000").unwrap();
    preview.command("tick 900001").unwrap();
    assert!(preview.text().contains("stale"));
}

#[test]
fn every_form_choice_commits_its_typed_state_and_round_trips_through_recording() {
    let mut preview = Preview::default();
    for (charging, expected) in [
        ("unknown", Charging::Unknown),
        ("charging", Charging::Charging),
        ("off", Charging::NotCharging),
    ] {
        preview
            .apply_measurements(&MeasurementInputs {
                charging,
                ..MeasurementInputs::default()
            })
            .unwrap();
        assert_eq!(preview.app().telemetry.charging, expected);
    }
    for (sd, expected) in [
        ("absent", StorageStatus::Absent),
        ("ready", StorageStatus::Ready),
        ("fault", StorageStatus::Fault),
    ] {
        preview
            .apply_measurements(&MeasurementInputs {
                sd,
                ..MeasurementInputs::default()
            })
            .unwrap();
        assert_eq!(preview.app().diagnostics.storage, expected);
    }
    for (radio, expected) in [
        ("off", RadioStatus::Off),
        ("connected", RadioStatus::Connected),
        ("disconnected", RadioStatus::Disconnected),
    ] {
        preview
            .apply_measurements(&MeasurementInputs {
                wifi: radio,
                ble: radio,
                ..MeasurementInputs::default()
            })
            .unwrap();
        assert_eq!(
            (
                preview.app().diagnostics.wifi,
                preview.app().diagnostics.ble
            ),
            (expected, expected)
        );
    }
    let mut replay = Preview::default();
    replay.replay(&preview.recording()).unwrap();
    assert_eq!(replay.app(), preview.app());
}
