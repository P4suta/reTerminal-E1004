use crate::presentation::Presentation;
use crate::{app::*, geometry::*, settings::*, telemetry::*};

#[kani::proof]
#[kani::unwind(5)]
fn panel_failure_revokes_sleep_and_stale_failures_cannot_change_state() {
    let mut app = App::default();
    app.sequence = kani::any();
    app.sleep_requested = kani::any();
    app.pending = kani::any();
    let id: u32 = kani::any();
    let failed = app.transition(AppEvent::PanelFailed {
        id,
        fault: Fault::Bus,
    });
    if id == app.sequence {
        assert_eq!(failed.state.refresh, RefreshState::Failed(Fault::Bus));
        assert_eq!(failed.state.diagnostics.last_error, Some(Fault::Bus));
        assert!(!failed.state.sleep_requested);
        assert!(failed.actions.contains(&Action::StopPanel));
        assert!(!failed
            .actions
            .iter()
            .any(|action| matches!(action, Action::Sleep { .. })));
    } else {
        let unchanged = app.transition(AppEvent::MeasurementBusy(app.measurement_busy));
        assert_eq!(failed, unchanged);
    }
}

#[kani::proof]
#[kani::unwind(2)]
fn presentation_accepts_only_the_active_success_and_cancel_revokes_completion() {
    let initial: u8 = kani::any();
    let next: u8 = kani::any();
    let id: u32 = kani::any();
    let completion: u32 = kani::any();
    let success: bool = kani::any();
    let cancelled: bool = kani::any();
    let mut display = Presentation::new(initial);
    display.start(id, next);
    if cancelled {
        display.cancel();
    }
    display.finish(completion, if success { Ok(()) } else { Err(Fault::Bus) });
    let accepted = !cancelled && id == completion;
    assert_eq!(
        display.shown(),
        if accepted && success { next } else { initial }
    );
    assert_eq!(
        display.active_id(),
        if !cancelled && !accepted {
            Some(id)
        } else {
            None
        }
    );
    let shown = display.shown();
    if accepted || cancelled {
        display.finish(id, Ok(()));
        assert_eq!(display.shown(), shown);
    }
    kani::cover!(accepted && success && initial != next);
    kani::cover!(cancelled && id == completion && initial != next);
}

#[kani::proof]
#[kani::unwind(10)]
fn decimal_parser_matches_signed_fixed_point_without_rounding() {
    let whole: u16 = kani::any();
    let fraction: u8 = kani::any();
    let negative: bool = kani::any();
    kani::assume(whole <= 999 && fraction < 100);
    let bytes = [
        if negative { b'-' } else { b'0' },
        b'0' + (whole / 100) as u8,
        b'0' + ((whole / 10) % 10) as u8,
        b'0' + (whole % 10) as u8,
        b'.',
        b'0' + fraction / 10,
        b'0' + fraction % 10,
    ];
    let text = core::str::from_utf8(&bytes).unwrap();
    let magnitude = i32::from(whole) * 100 + i32::from(fraction);
    assert_eq!(
        parse_hundredths(text),
        Some(if negative { -magnitude } else { magnitude })
    );
    kani::cover!(negative && whole == 0 && fraction == 1);
    kani::cover!(!negative && whole == 130 && fraction == 0);
}

#[kani::proof]
#[kani::unwind(10)]
fn decimal_parser_rejects_non_decimal_bytes_without_panicking() {
    let mut bytes: [u8; 8] = kani::any();
    for byte in &mut bytes {
        kani::assume(byte.is_ascii());
    }
    let position: usize = kani::any();
    kani::assume(position < bytes.len());
    bytes[position] = b'x';
    assert_eq!(
        parse_hundredths(core::str::from_utf8(&bytes).unwrap()),
        None
    );
}

#[kani::proof]
#[kani::unwind(2)]
fn rotation_and_address_are_bounded() {
    let selector: u8 = kani::any();
    let rotation = match selector % 4 {
        0 => Rotation::Portrait,
        1 => Rotation::Landscape,
        2 => Rotation::InvertedPortrait,
        _ => Rotation::InvertedLandscape,
    };
    let x: u16 = kani::any();
    let y: u16 = kani::any();
    let (width, height) = rotation.dimensions();
    let result = rotation.physical(x, y);
    assert_eq!(result.is_some(), x < width && y < height);
    if let Some((px, py)) = result {
        assert!(px < WIDTH && py < HEIGHT);
        let (index, high) = pixel_address(px, py).unwrap();
        assert!(index < FRAME_BYTES);
        assert_eq!(high, px % 2 == 0);
    }
    kani::cover!(result.is_some());
    kani::cover!(result.is_none());
}

#[kani::proof]
#[kani::unwind(2)]
fn nibble_write_preserves_the_other_pixel() {
    let byte: u8 = kani::any();
    let high: bool = kani::any();
    let color = match kani::any::<u8>() % 6 {
        0 => Color::Black,
        1 => Color::White,
        2 => Color::Yellow,
        3 => Color::Red,
        4 => Color::Blue,
        _ => Color::Green,
    };
    let changed = replace_nibble(byte, high, color);
    if high {
        assert_eq!(changed & 15, byte & 15);
        assert_eq!(changed >> 4, color as u8);
    } else {
        assert_eq!(changed >> 4, byte >> 4);
        assert_eq!(changed & 15, color as u8);
    }
}

#[kani::proof]
#[kani::unwind(2)]
fn controller_mapping_is_disjoint_and_bounded() {
    let row: u16 = kani::any();
    let col: u16 = kani::any();
    let a = controller_source(false, row, col);
    let b = controller_source(true, row, col);
    assert_eq!(a.is_some(), row < HEIGHT && col < WIDTH / 4);
    if let (Some(a), Some(b)) = (a, b) {
        assert_eq!(b - a, 300);
        assert!(a < FRAME_BYTES && b < FRAME_BYTES);
        assert_eq!(a / 600, usize::from(row));
        assert_eq!(b / 600, usize::from(row));
        kani::cover!(a == 0);
    }
}

#[kani::proof]
#[kani::unwind(9)]
fn battery_estimate_is_monotone_and_bounded() {
    let mv: u16 = kani::any();
    kani::assume((2000..5000).contains(&mv));
    let a = BatteryMillivolts::new(mv).unwrap().estimated_percent();
    let b = BatteryMillivolts::new(mv + 1).unwrap().estimated_percent();
    assert!(a <= b && b <= 100);
    kani::cover!(a > 0 && a < 100);
}

#[kani::proof]
#[kani::unwind(33)]
fn settings_round_trip_and_single_bit_corruption() {
    let refresh_seconds: u32 = kani::any();
    let rotation: u8 = kani::any();
    kani::assume((60..=86400).contains(&refresh_seconds) && rotation < 4);
    let settings = Settings {
        refresh_seconds,
        rotation,
    };
    let generation: u32 = kani::any();
    let bytes = encode(settings, generation);
    assert_eq!(decode(&bytes), Some((settings, generation)));
    let index: usize = kani::any();
    let bit: u8 = kani::any();
    kani::assume(index < SLOT_BYTES && bit < 8);
    let mut damaged = bytes;
    damaged[index] ^= 1 << bit;
    assert_eq!(decode(&damaged), None);
}

#[kani::proof]
#[kani::unwind(5)]
fn updating_never_sleeps_or_starts_a_second_display() {
    let mut app = App::default()
        .transition(AppEvent::Wake(WakeReason::PowerOn))
        .state;
    app.now_ms = kani::any();
    app.refresh = RefreshState::Updating {
        id: 1,
        started_ms: app.now_ms,
    };
    app.storage_busy = kani::any();
    app.screen = match kani::any::<u8>() % 3 {
        0 => ScreenId::Status,
        1 => ScreenId::Diagnostics,
        _ => ScreenId::Drawing,
    };
    let event = match kani::any::<u8>() % 6 {
        0 => AppEvent::Button(Button::Next),
        1 => AppEvent::Button(Button::Previous),
        2 => AppEvent::Button(Button::Refresh),
        3 => AppEvent::StorageChanged(StorageStatus::Ready),
        4 => AppEvent::RadioChanged {
            wifi: RadioStatus::Connected,
            ble: RadioStatus::Connected,
        },
        _ => AppEvent::RequestSleep,
    };
    let transition = app.transition(event);
    for action in transition.actions {
        assert!(!matches!(
            action,
            Action::Sleep { .. } | Action::Display { .. }
        ));
    }
    assert_eq!(transition.state.refresh, app.refresh);
}

#[kani::proof]
#[kani::unwind(5)]
fn timeout_never_becomes_success_or_sleep() {
    let mut app = App::default()
        .transition(AppEvent::Wake(WakeReason::Timer))
        .state;
    let now: u64 = kani::any();
    kani::assume(now >= DISPLAY_TIMEOUT_MS);
    app.sleep_requested = true;
    let failed = app.transition(AppEvent::Tick(now));
    assert_eq!(
        failed.state.refresh,
        RefreshState::Failed(Fault::PanelTimeout)
    );
    for action in failed.actions {
        assert!(!matches!(action, Action::Sleep { .. }));
    }
    let late = failed.state.transition(AppEvent::DisplayFinished {
        id: 1,
        result: Ok(()),
    });
    assert_eq!(
        late.state.refresh,
        RefreshState::Failed(Fault::PanelTimeout)
    );
    let retry = late
        .state
        .transition(AppEvent::Tick(now.saturating_add(REFRESH_INTERVAL_MS)));
    assert!(retry.actions.contains(&Action::Measure));
    assert!(retry.actions.contains(&Action::Display { id: 2 }));
}

#[kani::proof]
#[kani::unwind(2)]
fn rotation_is_injective() {
    let rotation = match kani::any::<u8>() % 4 {
        0 => Rotation::Portrait,
        1 => Rotation::Landscape,
        2 => Rotation::InvertedPortrait,
        _ => Rotation::InvertedLandscape,
    };
    let (x, y, other_x, other_y): (u16, u16, u16, u16) = kani::any();
    if let (Some(a), Some(b)) = (rotation.physical(x, y), rotation.physical(other_x, other_y)) {
        if a == b {
            assert_eq!((x, y), (other_x, other_y));
        }
        kani::cover!(a != b);
    }
}

#[kani::proof]
#[kani::unwind(5)]
fn sleep_requires_all_work_to_be_quiescent() {
    let mut app = App::default();
    app.measurement_busy = kani::any();
    app.storage_busy = kani::any();
    app.pending = kani::any();
    let transition = app.transition(AppEvent::RequestSleep);
    for action in transition.actions {
        if matches!(action, Action::Sleep { .. }) {
            assert!(
                !transition.state.measurement_busy
                    && !transition.state.storage_busy
                    && !transition.state.pending
            );
            assert_eq!(transition.state.refresh, RefreshState::Idle);
        }
    }
    kani::cover!(transition.actions.contains(&Action::Sleep { seconds: 900 }));
}
