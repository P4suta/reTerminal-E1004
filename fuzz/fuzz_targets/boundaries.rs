#![no_main]

use device_core::{app::*, geometry::*, settings::*, telemetry::*};
use e1004_bsp::sensors::*;
use libfuzzer_sys::fuzz_target;
#[path = "../../crates/device-runtime/tests/console_fuzz.rs"]
mod console_properties;

fuzz_target!(|data: &[u8]| {
    console_properties::check(data);
    let ssid = data
        .iter()
        .take(32)
        .map(|byte| char::from(b' ' + byte % 95))
        .collect::<String>();
    let password = data
        .iter()
        .rev()
        .take(63)
        .map(|byte| char::from(b' ' + byte % 95))
        .collect::<String>();
    if !ssid.is_empty() {
        for password in [String::new(), password] {
            let line = format!("wifi connect {ssid}\t{password}");
            assert_eq!(
                e1004_bsp::console::parse(&line),
                Ok(e1004_bsp::console::Command::WifiConnect {
                    ssid: &ssid,
                    password: &password
                })
            );
        }
    }
    let mut input = e1004_bsp::console::LineInput::default();
    for byte in data {
        input.push(*byte);
        let _ = input.line();
    }
    let _ = decode(data);
    let _ = recover(data, &data[..data.len() / 2]);
    if let Some(record) = device_core::media::ImageRecord::decode(data) {
        assert_eq!(
            device_core::media::ImageRecord::decode(&record.encode()),
            Some(record)
        );
    }
    for byte in data.iter().take(256) {
        assert_eq!(
            device_core::media::valid_palette_byte(*byte),
            matches!(byte >> 4, 0 | 1 | 2 | 3 | 5 | 6)
                && matches!(byte & 15, 0 | 1 | 2 | 3 | 5 | 6)
        );
    }
    if let Ok(text) = core::str::from_utf8(data) {
        if xtask::bindings::validate(text).is_ok() {
            use quote::ToTokens;
            let parsed = syn::parse_file(text).unwrap();
            assert!(xtask::bindings::validate(&parsed.to_token_stream().to_string()).is_ok());
        }
        let mut preview = device_preview::Preview::default();
        let before = (*preview.app(), preview.shown(), preview.recording());
        if preview.command(text).is_err() {
            assert_eq!(
                (*preview.app(), preview.shown(), preview.recording()),
                before
            );
        }
        let before = (*preview.app(), preview.shown(), preview.recording());
        if preview.replay(text).is_err() {
            assert_eq!(
                (*preview.app(), preview.shown(), preview.recording()),
                before
            );
        }
        let mut replay = device_preview::Preview::default();
        replay.replay(&preview.recording()).unwrap();
        assert_eq!(replay.app(), preview.app());
        assert_eq!(replay.shown(), preview.shown());
        if let Some(value) = parse_hundredths(text)
            .and_then(|n| i16::try_from(n).ok())
            .and_then(Temperature::new)
        {
            let n = i32::from(value.get());
            let canonical = format!(
                "{}{}.{:02}",
                if n < 0 { "-" } else { "" },
                n.abs() / 100,
                n.abs() % 100
            );
            assert_eq!(parse_hundredths(&canonical), Some(n));
        }
        for text in text.lines().chain(core::iter::once(text)) {
            let expected = ["a", "b"];
            if xtask::evidence::verify_test_inventory(text, &expected).is_ok() {
                let names = text
                    .lines()
                    .filter_map(|line| line.strip_suffix(": test"))
                    .collect::<Vec<_>>();
                assert_eq!(names.len(), expected.len());
                assert!(expected.iter().all(|name| names.contains(name)));
                let canonical = names
                    .iter()
                    .map(|name| format!("{name}: test\n"))
                    .collect::<String>();
                assert!(xtask::evidence::verify_test_inventory(&canonical, &expected).is_ok());
            }
            let files = ["crates/preview/src/lib.rs"];
            if let Ok(count) = xtask::evidence::validate_mutations(text, "27.1.0", &files) {
                assert!(count > 0);
                let canonical = serde_json::to_string(
                    &serde_json::from_str::<serde_json::Value>(text).unwrap(),
                )
                .unwrap();
                assert_eq!(
                    xtask::evidence::validate_mutations(&canonical, "27.1.0", &files),
                    Ok(count)
                );
            }
            if let Ok(artifacts) = xtask::evidence::result_artifacts(text) {
                assert!(!artifacts.is_empty());
                assert!(artifacts
                    .iter()
                    .all(|artifact| !artifact.contains('/') && !artifact.contains("..")));
            }
            if let Ok(command) = e1004_bsp::console::parse(text) {
                use e1004_bsp::console::Command;
                let canonical = match &command {
                    Command::Help => "help".into(),
                    Command::Status => "status".into(),
                    Command::Diagnose => "diagnose".into(),
                    Command::Refresh => "refresh".into(),
                    Command::Sleep => "sleep".into(),
                    Command::SetRtc(value) => format!("rtc {value}"),
                    Command::SetInterval(value) => format!("interval {value}"),
                    Command::Resume(policy) => match policy {
                        ResumePolicy::Last => "resume last".into(),
                        ResumePolicy::Status => "resume status".into(),
                    },
                    Command::Device(_) | Command::Media(_) => text.trim().to_owned(),
                    Command::WifiScan => "wifi scan".into(),
                    Command::WifiConnect { ssid, password } => {
                        format!("wifi connect {ssid}\t{password}")
                    }
                    Command::WifiDisconnect => "wifi disconnect".into(),
                    Command::Ble(enabled) => format!("ble {}", if *enabled { "on" } else { "off" }),
                };
                assert_eq!(e1004_bsp::console::parse(&canonical), Ok(command));
            }
            if let Ok(features) = xtask::FirmwareFeatures::parse(text) {
                use xtask::FirmwareFeatures;
                let canonical = match features {
                    FirmwareFeatures::Offline => "offline",
                    FirmwareFeatures::Wifi => "wifi",
                    FirmwareFeatures::Ble => "ble",
                    FirmwareFeatures::Both => "wifi,ble",
                };
                assert_eq!(text, canonical);
                assert_eq!(xtask::FirmwareFeatures::parse(canonical), Ok(features));
            }
            if let Some(path) = xtask::web_path(text) {
                assert!(path.starts_with("web"));
                assert!(!path
                    .components()
                    .any(|part| part == std::path::Component::ParentDir));
            }
            let _ = xtask::monochrome(text);
        }
        if let Some(time) = DateTime::parse(text) {
            assert_eq!(
                format!(
                    "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
                    time.year, time.month, time.day, time.hour, time.minute, time.second
                ),
                text
            );
        }
    }
    let mut frame = vec![0x11; FRAME_BYTES];
    let mut target = device_ui::Framebuffer::new(&mut frame, Rotation::Portrait).unwrap();
    let _ = device_ui::draw_bmp(&mut target, data, Default::default());
    if let Some(raw) = data.get(..9) {
        let settings = Settings {
            refresh_seconds: 60 + u32::from_le_bytes(raw[..4].try_into().unwrap()) % 86341,
            rotation: raw[4] % 4,
            resume: if raw[4] & 4 == 0 {
                ResumePolicy::Last
            } else {
                ResumePolicy::Status
            },
            screen: ScreenId::ALL[usize::from(raw[5]) % ScreenId::ALL.len()],
            ..Settings::default()
        };
        let generation = u32::from_le_bytes(raw[5..9].try_into().unwrap());
        assert_eq!(
            decode(&encode(settings, generation)),
            Some((settings, generation))
        );
    }
    if let Some(bytes) = data.get(..7) {
        if let Ok(time) = rtc_decode(bytes.try_into().unwrap()) {
            assert_eq!(
                rtc_decode(rtc_encode(time).unwrap()[1..].try_into().unwrap()),
                Ok(time)
            );
        }
        let mut sensor: [u8; 6] = bytes[..6].try_into().unwrap();
        let _ = sht40_decode(sensor);
        sensor[2] = sensor_crc([sensor[0], sensor[1]]);
        sensor[5] = sensor_crc([sensor[3], sensor[4]]);
        let (temperature, humidity) = sht40_decode(sensor).unwrap();
        assert!((-4500..=13000).contains(&temperature.get()));
        assert!(humidity.get() <= 10000);
    }
    let mut app = App::default();
    for byte in data.iter().take(32) {
        let event = match byte % 9 {
            8 => AppEvent::PanelFailed {
                id: app.sequence,
                fault: Fault::Bus,
            },
            0 => AppEvent::Wake(WakeReason::Timer),
            1 => AppEvent::Button(Button::Previous),
            2 => AppEvent::Button(Button::Next),
            3 => AppEvent::RequestSleep,
            4 => AppEvent::Tick(app.now_ms.saturating_add(u64::from(*byte) * 1000)),
            5 => AppEvent::StorageBusy(byte & 8 != 0),
            6 => AppEvent::DisplayFinished {
                id: app.sequence,
                result: Ok(()),
            },
            _ => AppEvent::DisplayFinished {
                id: app.sequence,
                result: Err(Fault::Bus),
            },
        };
        let transition = app.transition(event);
        for action in transition.actions {
            if matches!(action, Action::Sleep { .. }) {
                assert_eq!(transition.state.refresh, RefreshState::Idle);
                assert!(
                    !transition.state.pending
                        && !transition.state.storage_busy
                        && !transition.state.measurement_busy
                );
            }
        }
        app = transition.state;
        let x = u16::from(*byte) * 7;
        if let Some((index, _)) = pixel_address(x, x) {
            assert!(index < FRAME_BYTES);
        }
    }
});
