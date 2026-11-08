use crate::presentation::Presentation;
use crate::{app::*, geometry::*, settings::*, telemetry::*};

#[kani::proof]
#[kani::unwind(513)]
fn retained_selection_requires_matching_preferences_and_a_sleep_wake() {
    let base = Settings::default();
    let selected: u8 = kani::any();
    kani::assume((1..=8).contains(&selected));
    let mut retained = Settings {
        rotation: kani::any(),
        refresh_seconds: kani::any(),
        gallery: crate::media::GallerySettings {
            selected: crate::media::ImageId::new(selected).unwrap(),
            ..Default::default()
        },
        ..base
    };
    let changed: bool = kani::any();
    if changed {
        retained.daily = retained
            .daily
            .edit(DailyEdit::Note(Text::new("Changed").unwrap()));
    }
    let wake = match kani::any::<u8>() % 3 {
        0 => WakeReason::PowerOn,
        1 => WakeReason::Button,
        _ => WakeReason::Timer,
    };
    let expected = (retained.rotation == 0
        && retained.refresh_seconds == 900
        && !changed
        && wake != WakeReason::PowerOn)
        .then_some(retained.gallery.selected);
    assert_eq!(retained.retained_selection(base, wake), expected);
    kani::cover!(expected.is_some());
    kani::cover!(expected.is_none() && wake == WakeReason::Timer);
}

#[kani::proof]
#[kani::unwind(10)]
fn packed_palette_and_transfer_admission_match_wire_contract() {
    use crate::media::*;
    let byte: u8 = kani::any();
    let legal = |code| matches!(code, 0 | 1 | 2 | 3 | 5 | 6);
    assert_eq!(
        valid_palette_byte(byte),
        legal(byte >> 4) && legal(byte & 15)
    );
    let transfer = Transfer::new(
        17,
        PhysicalSlot::new(0).unwrap(),
        ImageRecord::image(ImageId::default(), 0, Rotation::Portrait, 0),
    )
    .unwrap();
    let offset: u32 = kani::any();
    let result = transfer.plan(17, offset as usize, &[byte; 4]);
    if let Ok(WritePlan::Append(next)) = result {
        assert_eq!(offset, 0);
        assert!(valid_palette_byte(byte));
        assert_eq!(next.received(), 4);
    }
    assert_eq!(transfer.plan(18, 0, &[0; 4]), Err(TransferError::Identity));
    assert_eq!(transfer.complete(17), Err(TransferError::Incomplete));
    kani::cover!(result.is_ok());
    kani::cover!(result.is_err());
}

#[kani::proof]
fn image_placement_returns_only_bounded_source_coordinates() {
    use crate::image::{Anchor, Fit, Placement, Size};
    let fit = u8::from(kani::any::<bool>()) + 2 * u8::from(kani::any::<bool>());
    let anchor: u8 = kani::any();
    kani::assume(anchor < 9);
    let x: u8 = kani::any();
    let y: u8 = kani::any();
    let source = Size::new(3, 5).unwrap();
    let target = Size::new(12, 16).unwrap();
    let placement = Placement::new(
        source,
        target,
        Fit::from_code(fit).unwrap(),
        Anchor::from_code(anchor).unwrap(),
    );
    if let Some((sx, sy)) = placement.source(u16::from(x), u16::from(y)) {
        assert!(sx < 3 && sy < 5 && x < 12 && y < 16);
    }
    kani::cover!(placement.source(u16::from(x), u16::from(y)).is_some());
    kani::cover!(placement.source(u16::from(x), u16::from(y)).is_none());
}

#[kani::proof]
#[kani::unwind(513)]
fn configuration_acknowledgement_preserves_a_different_request() {
    let requested = Configuration {
        refresh_seconds: kani::any(),
        rotation: kani::any(),
        resume: if kani::any() {
            ResumePolicy::Last
        } else {
            ResumePolicy::Status
        },
    };
    let id = ConfigurationId::new(kani::any());
    let saved = ConfigurationId::new(kani::any());
    let request = ConfigurationRequest::new(id, requested);
    assert_eq!(
        request.acknowledge(saved),
        if id == saved { None } else { Some(request) }
    );
    let base = Settings {
        daily: Daily::default().edit(DailyEdit::Note(Text::new("Keep").unwrap())),
        ..Settings::default()
    };
    let projected = base.with_configuration(requested, ScreenId::Checklist);
    assert_eq!(projected.configuration(), requested);
    assert_eq!(projected.daily, base.daily);
    assert_eq!(
        projected.screen,
        if requested.resume == ResumePolicy::Last {
            ScreenId::Checklist
        } else {
            ScreenId::Status
        }
    );
}

#[kani::proof]
#[kani::unwind(513)]
fn saved_content_never_replaces_a_reverted_edit() {
    let edit = if kani::any() {
        DailyEdit::Note(Text::new("Changed").unwrap())
    } else {
        DailyEdit::SetTask {
            id: TaskId::default(),
            text: Text::new("Changed").unwrap(),
        }
    };
    let original = App::default();
    let saving = original
        .transition(AppEvent::EditDaily(edit))
        .state
        .settings();
    let acknowledged = original.transition(AppEvent::Configure(saving)).state;
    assert_eq!(acknowledged.daily, original.daily);
    assert_eq!(acknowledged.persisted_settings, saving);
}
use crate::{
    calendar::Month,
    daily::{Daily, DailyEdit, TaskId, Text, TASK_COUNT},
};

#[kani::proof]
#[kani::unwind(9)]
fn direct_calendar_month_parser_and_selection_preserve_the_domain() {
    let bytes: [u8; 7] = kani::any();
    if let Ok(text) = core::str::from_utf8(&bytes) {
        let digits = [bytes[0], bytes[1], bytes[2], bytes[3], bytes[5], bytes[6]]
            .into_iter()
            .all(|digit| digit.is_ascii_digit());
        let year = u32::from(bytes[0].wrapping_sub(b'0')) * 1000
            + u32::from(bytes[1].wrapping_sub(b'0')) * 100
            + u32::from(bytes[2].wrapping_sub(b'0')) * 10
            + u32::from(bytes[3].wrapping_sub(b'0'));
        let number =
            u32::from(bytes[5].wrapping_sub(b'0')) * 10 + u32::from(bytes[6].wrapping_sub(b'0'));
        let valid = digits
            && bytes[4] == b'-'
            && (2000..=2099).contains(&year)
            && (1..=12).contains(&number);
        let month = Month::parse(text);
        assert_eq!(month.is_some(), valid);
        if let Some(month) = month {
            assert_eq!(u32::from(month.year()), year);
            assert_eq!(u32::from(month.number()), number);
            kani::cover!(month.year() == 2000 && month.number() == 1);
            kani::cover!(month.year() == 2099 && month.number() == 12);
        }
        kani::cover!(!valid);
    }
}

#[kani::proof]
#[kani::unwind(161)]
fn direct_calendar_selection_preserves_other_daily_content() {
    let year: u16 = kani::any();
    let number: u8 = kani::any();
    let today: bool = kani::any();
    if let Some(month) = Month::new(year, number) {
        let selected = (!today).then_some(month);
        let app = App::default();
        let changed = app
            .transition(AppEvent::SelectCalendarMonth(selected))
            .state;
        assert_eq!(changed.screen, crate::app::ScreenId::Calendar);
        assert_eq!(changed.calendar_month, selected);
        assert_eq!(changed.daily, app.daily);
        kani::cover!(today);
        kani::cover!(!today);
    }
}

#[kani::proof]
#[kani::unwind(3)]
fn calendar_cells_and_navigation_are_bounded() {
    let year: u16 = kani::any();
    let number: u8 = kani::any();
    let cell: u8 = kani::any();
    if let Some(month) = Month::new(year, number) {
        assert!(month.first_weekday() < 7);
        if let Some(day) = month.day_at(cell) {
            assert!(day >= 1 && day <= month.days());
        }
        for forward in [false, true] {
            let changed = month.step(forward);
            assert_eq!(Month::new(changed.year(), changed.number()), Some(changed));
        }
        kani::cover!(month.number() == 2 && month.days() == 29);
    }
}

#[kani::proof]
fn task_selection_stays_in_range_and_is_reversible() {
    let number: u8 = kani::any();
    if let Some(id) = TaskId::new(number) {
        assert!(id.index() < TASK_COUNT);
        assert_eq!(id.step(true).step(false), id);
        assert_eq!(id.step(false).step(true), id);
        kani::cover!(number == 6);
    }
}

#[kani::proof]
#[kani::unwind(6)]
fn printable_text_is_bounded_and_rejects_control_bytes() {
    let bytes: [u8; 4] = kani::any();
    if let Ok(input) = core::str::from_utf8(&bytes) {
        let result = Text::<4>::new(input);
        assert_eq!(
            result.is_some(),
            bytes.iter().all(|byte| (b' '..=b'~').contains(byte))
        );
        if let Some(text) = result {
            assert!(text.as_str().len() <= 4);
        }
    }
}

#[kani::proof]
#[kani::unwind(513)]
fn daily_edits_round_trip_and_preserve_unselected_tasks() {
    let number: u8 = kani::any();
    kani::assume((1..=6).contains(&number));
    let id = TaskId::new(number).unwrap();
    let done: bool = kani::any();
    let daily = Daily::default()
        .edit(DailyEdit::SetTask {
            id,
            text: Text::new("Task").unwrap(),
        })
        .edit(DailyEdit::SetDone { id, done });
    assert_eq!(daily.task(id).done, done);
    assert_eq!(daily.edit(DailyEdit::Toggle(id)).task(id).done, !done);
    for (index, task) in daily.tasks().iter().enumerate() {
        if index != id.index() {
            assert!(task.text.is_empty() && !task.done);
        }
    }
    assert_eq!(Daily::decode(&daily.encode()), Some(daily));
    assert_eq!(daily.edit(DailyEdit::Clear(id)), Daily::default());
}

#[kani::proof]
#[kani::unwind(513)]
fn acknowledged_screen_policy_preserves_navigation_and_deadline() {
    let refresh_seconds: u32 = kani::any();
    kani::assume((MIN_REFRESH_SECONDS..=MAX_REFRESH_SECONDS).contains(&refresh_seconds));
    let app = App {
        refresh_seconds,
        screen: match kani::any::<u8>() % 3 {
            0 => ScreenId::Status,
            1 => ScreenId::Diagnostics,
            _ => ScreenId::Drawing,
        },
        resume: if kani::any() {
            ResumePolicy::Last
        } else {
            ResumePolicy::Status
        },
        rotation: match kani::any::<u8>() % 4 {
            0 => Rotation::Portrait,
            1 => Rotation::Landscape,
            2 => Rotation::InvertedPortrait,
            _ => Rotation::InvertedLandscape,
        },
        next_refresh_ms: kani::any(),
        now_ms: kani::any(),
        ..App::default()
    };
    let desired = app.settings();
    assert_eq!(desired.refresh_seconds, app.refresh_seconds);
    assert_eq!(desired.rotation, app.rotation as u8);
    assert_eq!(desired.resume, app.resume);
    assert_eq!(
        desired.screen,
        if app.resume == ResumePolicy::Last {
            app.screen
        } else {
            ScreenId::Status
        }
    );
    assert_eq!(desired.wake_screen(), desired.screen);
    let resume = if kani::any() {
        ResumePolicy::Last
    } else {
        ResumePolicy::Status
    };
    let desired = Settings {
        resume,
        screen: Settings { resume, ..desired }.wake_screen(),
        ..desired
    };
    let saved = app.transition(AppEvent::Configure(desired));
    assert_eq!(saved.state.screen, app.screen);
    assert_eq!(saved.state.persisted_settings, desired);
    assert_eq!(saved.state.next_refresh_ms, app.next_refresh_ms);
    assert_eq!(saved.actions, [Action::None; 3]);
    kani::cover!(app.resume == ResumePolicy::Status && app.screen == ScreenId::Drawing);
    kani::cover!(desired.resume != app.resume);
}

#[kani::proof]
#[kani::unwind(513)]
fn previous_settings_format_preserves_values_and_rejects_reserved_payload() {
    let seconds: u32 = kani::any();
    let rotation: u8 = kani::any();
    kani::assume(
        (MIN_REFRESH_SECONDS..=MAX_REFRESH_SECONDS).contains(&seconds) && rotation < ROTATION_COUNT,
    );
    let settings = Settings {
        refresh_seconds: seconds,
        rotation,
        ..Settings::default()
    };
    let generation: u32 = kani::any();
    let mut bytes = [0u8; LEGACY_SLOT_BYTES];
    bytes[..4].copy_from_slice(b"E104");
    bytes[4] = 1;
    bytes[5] = rotation;
    bytes[8..12].copy_from_slice(&generation.to_le_bytes());
    bytes[12..16].copy_from_slice(&seconds.to_le_bytes());
    let crc = crc32(&bytes[..16]);
    bytes[16..20].copy_from_slice(&crc.to_le_bytes());
    bytes[20..24].copy_from_slice(b"DONE");
    assert_eq!(decode(&bytes), Some((settings, generation)));
    let reserved: u8 = kani::any();
    kani::assume(reserved != 0);
    let offset = if kani::any() { 6 } else { 7 };
    bytes[offset] = reserved;
    let crc = crc32(&bytes[..16]);
    bytes[16..20].copy_from_slice(&crc.to_le_bytes());
    assert_eq!(decode(&bytes), None);
}

#[kani::proof]
#[kani::unwind(2049)]
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
#[kani::unwind(513)]
fn settings_round_trip_preserves_values() {
    let refresh_seconds: u32 = kani::any();
    let rotation: u8 = kani::any();
    kani::assume((60..=86400).contains(&refresh_seconds) && rotation < 4);
    let settings = Settings {
        refresh_seconds,
        rotation,
        resume: if kani::any() {
            ResumePolicy::Last
        } else {
            ResumePolicy::Status
        },
        screen: match kani::any::<u8>() % 3 {
            0 => ScreenId::Status,
            1 => ScreenId::Diagnostics,
            _ => ScreenId::Drawing,
        },
        ..Settings::default()
    };
    let generation: u32 = kani::any();
    let bytes = encode(settings, generation);
    assert_eq!(decode(&bytes), Some((settings, generation)));
}

#[kani::proof]
#[kani::unwind(513)]
fn settings_envelope_rejects_single_bit_corruption() {
    let refresh_seconds: u32 = kani::any();
    let rotation: u8 = kani::any();
    kani::assume((60..=86400).contains(&refresh_seconds) && rotation < 4);
    let settings = Settings {
        refresh_seconds,
        rotation,
        resume: if kani::any() {
            ResumePolicy::Last
        } else {
            ResumePolicy::Status
        },
        screen: ScreenId::ALL[usize::from(kani::any::<u8>() % 6)],
        ..Settings::default()
    };
    let mut damaged = encode(settings, kani::any());
    let index: usize = kani::any();
    let bit: u8 = kani::any();
    kani::assume(index < SLOT_BYTES && bit < 8);
    for (position, byte) in damaged.iter_mut().enumerate() {
        if position == index {
            *byte ^= 1 << bit;
        }
    }
    assert!(validate_record(&damaged).is_none());
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
        .state
        .transition(AppEvent::Telemetry(TelemetrySnapshot::default()))
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
    assert_eq!(retry.state.refresh, RefreshState::Idle);
    let retry = retry
        .state
        .transition(AppEvent::Telemetry(TelemetrySnapshot::default()));
    assert!(retry.actions.contains(&Action::Display { id: 2 }));
}

#[kani::proof]
#[kani::unwind(5)]
fn measurement_barrier_cannot_emit_display_or_sleep() {
    let app = App {
        measurement_busy: true,
        pending: kani::any(),
        sleep_requested: kani::any(),
        sequence: kani::any(),
        ..App::default()
    };
    let event = match kani::any::<u8>() % 6 {
        0 => AppEvent::Wake(WakeReason::Timer),
        1 => AppEvent::Button(Button::Refresh),
        2 => AppEvent::Tick(kani::any()),
        3 => AppEvent::Button(Button::Next),
        4 => AppEvent::RequestSleep,
        _ => AppEvent::SettingsFailed,
    };
    let next = app.transition(event);
    assert!(next.state.measurement_busy);
    assert!(!next.actions.iter().any(|action| matches!(
        action,
        Action::Measure | Action::Display { .. } | Action::Sleep { .. }
    )));
    let ready = next
        .state
        .transition(AppEvent::Telemetry(TelemetrySnapshot::default()));
    assert!(!ready.state.measurement_busy);
    if next.state.pending {
        assert!(ready.actions.contains(&Action::Display {
            id: app.sequence.wrapping_add(1)
        }));
    }
    kani::cover!(next.state.pending && app.sequence == u32::MAX);
}

#[kani::proof]
#[kani::unwind(2049)]
fn settings_faults_preserve_the_sd_status() {
    let sequence = kani::any();
    let app = App {
        sequence,
        refresh: match kani::any::<u8>() % 3 {
            0 => RefreshState::Idle,
            1 => RefreshState::Updating {
                id: sequence,
                started_ms: kani::any(),
            },
            _ => RefreshState::Failed(Fault::Bus),
        },
        measurement_busy: kani::any(),
        storage_busy: kani::any(),
        screen: match kani::any::<u8>() % 3 {
            0 => ScreenId::Status,
            1 => ScreenId::Diagnostics,
            _ => ScreenId::Drawing,
        },
        diagnostics: Diagnostics {
            storage: match kani::any::<u8>() % 3 {
                0 => StorageStatus::Absent,
                1 => StorageStatus::Ready,
                _ => StorageStatus::Fault,
            },
            ..Diagnostics::default()
        },
        ..App::default()
    };
    let next = app.transition(AppEvent::SettingsFailed);
    assert_eq!(next.state.diagnostics.storage, app.diagnostics.storage);
    assert_eq!(
        next.state.diagnostics.last_error,
        Some(Fault::SettingsStorage)
    );
    assert!(next.actions.contains(&Action::Log(Fault::SettingsStorage)));
    assert_eq!(next.state.persisted_settings, app.persisted_settings);
    let displays = app.screen == ScreenId::Diagnostics
        && !app.measurement_busy
        && app.refresh == RefreshState::Idle;
    assert_eq!(
        next.actions
            .iter()
            .any(|action| matches!(action, Action::Display { .. })),
        displays,
    );
    assert_eq!(
        next.state.pending,
        app.screen == ScreenId::Diagnostics && !displays
    );
    assert_eq!(next.state.measurement_busy, app.measurement_busy);
    if !displays {
        assert_eq!(next.state.refresh, app.refresh);
    }
    kani::cover!(app.diagnostics.storage == StorageStatus::Ready);
    kani::cover!(displays);
    kani::cover!(next.state.pending);
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
