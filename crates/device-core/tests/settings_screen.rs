use device_core::geometry::Rotation;
use device_core::{app::*, settings::*};

#[test]
fn persisted_settings_project_the_active_policy_screen_rotation_and_interval() {
    for (resume, screen, expected_screen) in [
        (ResumePolicy::Last, ScreenId::Status, ScreenId::Status),
        (
            ResumePolicy::Last,
            ScreenId::Diagnostics,
            ScreenId::Diagnostics,
        ),
        (ResumePolicy::Last, ScreenId::Drawing, ScreenId::Drawing),
        (ResumePolicy::Status, ScreenId::Status, ScreenId::Status),
        (
            ResumePolicy::Status,
            ScreenId::Diagnostics,
            ScreenId::Status,
        ),
        (ResumePolicy::Status, ScreenId::Drawing, ScreenId::Status),
    ] {
        for rotation in [
            Rotation::Portrait,
            Rotation::Landscape,
            Rotation::InvertedPortrait,
            Rotation::InvertedLandscape,
        ] {
            for refresh_seconds in [
                MIN_REFRESH_SECONDS,
                DEFAULT_REFRESH_SECONDS,
                MAX_REFRESH_SECONDS,
            ] {
                let app = App {
                    resume,
                    screen,
                    rotation,
                    refresh_seconds,
                    ..App::default()
                };
                assert_eq!(
                    app.settings(),
                    Settings {
                        resume,
                        screen: expected_screen,
                        rotation: rotation as u8,
                        refresh_seconds,
                        ..Settings::default()
                    }
                );
            }
        }
    }
}

#[test]
fn changing_only_resume_policy_preserves_the_deadline_and_needs_no_display() {
    let app = App {
        screen: ScreenId::Drawing,
        now_ms: 1234,
        next_refresh_ms: 7777,
        ..App::default()
    };
    let settings = Settings {
        resume: ResumePolicy::Status,
        screen: ScreenId::Status,
        ..app.settings()
    };
    let configured = app.transition(AppEvent::Configure(settings));
    assert_eq!(configured.state.resume, ResumePolicy::Status);
    assert_eq!(configured.state.screen, ScreenId::Drawing);
    assert_eq!(configured.state.next_refresh_ms, app.next_refresh_ms);
    assert_eq!(configured.actions, [Action::None; 3]);
}

#[test]
fn screen_settings_round_trip_and_the_previous_format_defaults_without_data_loss() {
    for resume in [ResumePolicy::Last, ResumePolicy::Status] {
        for screen in ScreenId::ALL {
            for rotation in 0..ROTATION_COUNT {
                let settings = Settings {
                    resume,
                    screen,
                    rotation,
                    refresh_seconds: MAX_REFRESH_SECONDS,
                    ..Settings::default()
                };
                for generation in [0, u32::MAX] {
                    let encoded = encode(settings, generation);
                    assert_eq!(encoded[4], 4);
                    assert_eq!(decode(&encoded), Some((settings, generation)));
                    assert_eq!(
                        settings.wake_screen(),
                        if resume == ResumePolicy::Last {
                            screen
                        } else {
                            ScreenId::Status
                        }
                    );
                }
            }
        }
    }
    let settings = Settings {
        refresh_seconds: 1800,
        rotation: 3,
        ..Settings::default()
    };
    let mut previous: [u8; LEGACY_SLOT_BYTES] =
        encode(settings, 7)[..LEGACY_SLOT_BYTES].try_into().unwrap();
    previous[4] = 1;
    let crc = crc32(&previous[..16]);
    previous[16..20].copy_from_slice(&crc.to_le_bytes());
    previous[20..24].copy_from_slice(b"DONE");
    assert_eq!(decode(&previous), Some((settings, 7)));
    for offset in [6, 7] {
        let mut invalid = previous;
        invalid[offset] = 1;
        let crc = crc32(&invalid[..16]);
        invalid[16..20].copy_from_slice(&crc.to_le_bytes());
        assert_eq!(decode(&invalid), None);
    }
}

#[test]
fn acknowledged_screen_metadata_does_not_move_the_active_screen_or_refresh_deadline() {
    let app = App {
        screen: ScreenId::Drawing,
        now_ms: 1234,
        next_refresh_ms: 7777,
        ..App::default()
    };
    let desired = app.settings();
    let saved = app.transition(AppEvent::Configure(desired));
    assert_eq!(saved.state.screen, ScreenId::Drawing);
    assert_eq!(saved.state.persisted_settings, desired);
    assert_eq!(saved.state.next_refresh_ms, app.next_refresh_ms);
    assert_eq!(saved.actions, [Action::None; 3]);
}

#[test]
fn both_legacy_formats_read_real_records_and_reject_malformed_envelopes() {
    for version in [1, 2] {
        for code in 0..=5 {
            let mut bytes = [0u8; LEGACY_SLOT_BYTES];
            bytes[..4].copy_from_slice(b"E104");
            bytes[4] = version;
            bytes[7] = code;
            bytes[8..12].copy_from_slice(&7u32.to_le_bytes());
            bytes[12..16].copy_from_slice(&900u32.to_le_bytes());
            let crc = crc32(&bytes[..16]);
            bytes[16..20].copy_from_slice(&crc.to_le_bytes());
            bytes[20..24].copy_from_slice(b"DONE");
            let expected = if (version == 1 && code == 0) || (version == 2 && code <= 2) {
                Some((
                    Settings {
                        screen: ScreenId::from_code(code).unwrap(),
                        ..Settings::default()
                    },
                    7,
                ))
            } else {
                None
            };
            assert_eq!(decode(&bytes), expected);
            let mut padded = [0xff; SLOT_BYTES];
            padded[..LEGACY_SLOT_BYTES].copy_from_slice(&bytes);
            assert_eq!(decode(&padded), expected);
            if expected.is_some() {
                for length in [LEGACY_SLOT_BYTES + 1, SLOT_BYTES - 1, SLOT_BYTES + 1] {
                    let mut invalid = vec![0xff; length];
                    invalid[..LEGACY_SLOT_BYTES].copy_from_slice(&bytes);
                    assert_eq!(decode(&invalid), None);
                }
            }
        }
    }
    let mut truncated = [0; LEGACY_SLOT_BYTES];
    truncated[..4].copy_from_slice(b"E104");
    truncated[4] = 3;
    assert_eq!(decode(&truncated), None);
    let mut invalid_magic = encode(Settings::default(), 7);
    invalid_magic[0] = b'?';
    let checksum_offset = COMMIT_OFFSET - 4;
    let crc = crc32(&invalid_magic[..checksum_offset]);
    invalid_magic[checksum_offset..COMMIT_OFFSET].copy_from_slice(&crc.to_le_bytes());
    assert_eq!(decode(&invalid_magic), None);
}
