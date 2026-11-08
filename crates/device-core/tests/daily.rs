use device_core::{
    app::{Action, App, AppEvent, Button, ScreenId},
    calendar::Month,
    commands,
    daily::{Daily, DailyEdit, TaskId, Text, DAILY_BYTES, NOTE_TEXT_BYTES, TASK_TEXT_BYTES},
    settings::{decode, encode, Settings, SLOT_BYTES},
};

#[test]
fn calendar_matches_an_independent_day_count_for_the_entire_rtc_century() {
    let mut weekday = 5;
    let names = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let weekdays = [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ];
    for year in 2000..=2099 {
        for number in 1..=12 {
            let month = Month::new(year, number).unwrap();
            assert_eq!(month.first_weekday(), weekday, "{year}-{number}");
            assert_eq!(month.name(), names[usize::from(number - 1)]);
            let days = match number {
                2 if year % 4 == 0 => 29,
                2 => 28,
                4 | 6 | 9 | 11 => 30,
                _ => 31,
            };
            assert_eq!(month.days(), days);
            assert_eq!(month.weekday(0), None);
            assert_eq!(month.weekday(days + 1), None);
            let actual: Vec<_> = (0..42).filter_map(|cell| month.day_at(cell)).collect();
            assert_eq!(actual, (1..=days).collect::<Vec<_>>());
            for day in 1..=days {
                assert_eq!(month.weekday(day), Some(weekdays[usize::from(weekday)]));
                weekday = (weekday + 1) % 7;
            }
            if (year, number) != (2099, 12) {
                assert_eq!(month.step(true).step(false), month);
            }
            if (year, number) != (2000, 1) {
                assert_eq!(month.step(false).step(true), month);
            }
        }
    }
    assert_eq!(
        Month::new(2000, 1).unwrap().step(false),
        Month::new(2000, 1).unwrap()
    );
    assert_eq!(
        Month::new(2099, 12).unwrap().step(true),
        Month::new(2099, 12).unwrap()
    );
    for (year, month) in [(0, 0), (1999, 12), (2100, 1), (2026, 0), (2026, 13)] {
        assert!(Month::new(year, month).is_none());
    }
}

#[test]
fn daily_editing_preserves_other_slots_and_resets_completion_only_when_text_changes() {
    for number in 1..=6 {
        let id = TaskId::new(number).unwrap();
        assert_eq!(id.step(true).step(false), id);
        assert_eq!(id.step(false).step(true), id);
        let set = DailyEdit::SetTask {
            id,
            text: Text::new("Buy groceries").unwrap(),
        };
        let daily = Daily::default().edit(DailyEdit::SetDone { id, done: true });
        assert_eq!(daily.counts(), (0, 0));
        assert_eq!(daily.edit(DailyEdit::Toggle(id)), daily);
        let daily = daily.edit(set).edit(DailyEdit::Toggle(id));
        assert_eq!(daily.counts(), (1, 1));
        assert_eq!(daily.edit(set), daily);
        let changed = daily.edit(DailyEdit::SetTask {
            id,
            text: Text::new("Water plants").unwrap(),
        });
        assert_eq!(changed.counts(), (0, 1));
        assert_eq!(changed.edit(DailyEdit::Clear(id)), Daily::default());
        for other in 1..=6 {
            if other != number {
                assert!(daily.task(TaskId::new(other).unwrap()).text.is_empty());
            }
        }
        assert_eq!(Daily::decode(&daily.encode()), Some(daily));
    }
    assert!(TaskId::new(0).is_none() && TaskId::new(7).is_none());
    let daily = Daily::default();
    assert_eq!(Daily::decode(&[0; DAILY_BYTES - 1]), None);
    let mut invalid = daily.encode();
    invalid[0] = 1;
    assert_eq!(Daily::decode(&invalid), None);
    invalid[0] = 2;
    assert_eq!(Daily::decode(&invalid), None);
}

#[test]
fn text_boundaries_reject_unrenderable_bytes_and_noncanonical_storage() {
    assert!(Text::<512>::new(&"x".repeat(255)).is_some());
    assert!(Text::<512>::new(&"x".repeat(256)).is_none());
    assert!(Text::<TASK_TEXT_BYTES>::new(&"x".repeat(TASK_TEXT_BYTES)).is_some());
    assert!(Text::<TASK_TEXT_BYTES>::new(&"x".repeat(TASK_TEXT_BYTES + 1)).is_none());
    assert!(Text::<NOTE_TEXT_BYTES>::new(&"x".repeat(NOTE_TEXT_BYTES)).is_some());
    assert!(Text::<NOTE_TEXT_BYTES>::new(&"x".repeat(NOTE_TEXT_BYTES + 1)).is_none());
    assert_eq!(
        Text::<48>::new("  Buy milk  ").unwrap().as_str(),
        "Buy milk"
    );
    for byte in 0..=u8::MAX {
        let raw = [byte];
        if let Ok(value) = core::str::from_utf8(&raw) {
            assert_eq!(Text::<48>::new(value).is_some(), (32..=126).contains(&byte));
        }
    }
    for input in ["hello\nworld", "hello\tworld", "日本語", "é", "\0"] {
        assert!(Text::<160>::new(input).is_none());
    }
    let mut invalid = Daily::default().encode();
    invalid[2] = b'x';
    assert_eq!(Daily::decode(&invalid), None);
    invalid[1] = 49;
    assert_eq!(Daily::decode(&invalid), None);
    for (length, data) in [(1, 2), (300, 301)] {
        for byte in 0..=u8::MAX {
            let mut encoded = [0; DAILY_BYTES];
            encoded[length] = 1;
            encoded[data] = byte;
            assert_eq!(
                Daily::decode(&encoded).is_some(),
                (33..=126).contains(&byte)
            );
        }
        for (first, last) in [(b' ', b'x'), (b'x', b' ')] {
            let mut encoded = [0; DAILY_BYTES];
            encoded[length] = 2;
            encoded[data] = first;
            encoded[data + 1] = last;
            assert_eq!(Daily::decode(&encoded), None);
        }
    }
}

#[test]
fn shared_commands_have_exact_screen_edit_and_rotation_meanings() {
    use device_core::{commands::DeviceCommand, geometry::Rotation};
    let screens = [
        ("status", ScreenId::Status, "Device Status"),
        ("calendar", ScreenId::Calendar, "Calendar"),
        ("checklist", ScreenId::Checklist, "Checklist"),
        ("notes", ScreenId::Notes, "Notes"),
        ("diagnostics", ScreenId::Diagnostics, "Diagnostics"),
        ("drawing", ScreenId::Drawing, "Drawing"),
    ];
    for (name, screen, label) in screens {
        assert_eq!(screen.name(), label);
        assert_eq!(ScreenId::parse(name), Some(screen));
        assert_eq!(
            commands::parse(&format!("screen {name}")),
            Some(Ok(DeviceCommand::Screen(screen)))
        );
    }
    for (text, rotation) in [
        ("0", Rotation::Portrait),
        ("1", Rotation::Landscape),
        ("2", Rotation::InvertedPortrait),
        ("3", Rotation::InvertedLandscape),
    ] {
        assert_eq!(
            commands::parse(&format!("rotation {text}"))
                .unwrap()
                .unwrap()
                .into_event(),
            Err(rotation)
        );
    }
    let id = TaskId::new(6).unwrap();
    for (command, edit) in [
        (
            "task set 6 Buy groceries",
            DailyEdit::SetTask {
                id,
                text: Text::new("Buy groceries").unwrap(),
            },
        ),
        ("task done 6", DailyEdit::SetDone { id, done: true }),
        ("task open 6", DailyEdit::SetDone { id, done: false }),
        ("task toggle 6", DailyEdit::Toggle(id)),
        ("task clear 6", DailyEdit::Clear(id)),
        (
            "note set Library books.",
            DailyEdit::Note(Text::new("Library books.").unwrap()),
        ),
        (
            "note set clear",
            DailyEdit::Note(Text::new("clear").unwrap()),
        ),
        (
            "note set set alarm",
            DailyEdit::Note(Text::new("set alarm").unwrap()),
        ),
        ("note clear", DailyEdit::Note(Text::default())),
    ] {
        assert_eq!(
            commands::parse(command).unwrap().unwrap().into_event(),
            Ok(AppEvent::EditDaily(edit))
        );
    }
    for (command, button) in [
        ("hold previous", Button::Previous),
        ("hold refresh", Button::Refresh),
        ("hold next", Button::Next),
    ] {
        assert_eq!(
            commands::parse(command).unwrap().unwrap().into_event(),
            Ok(AppEvent::Hold(button))
        );
    }
    assert!(commands::parse("status").is_none());
}

#[test]
fn direct_selection_changes_each_screen_once_and_ignores_reselecting_it() {
    for start in ScreenId::ALL {
        for target in ScreenId::ALL {
            let app = App {
                screen: start,
                ..App::default()
            };
            let selected = app.transition(AppEvent::SelectScreen(target));
            assert_eq!(selected.state.screen, target);
            assert_eq!(
                selected
                    .actions
                    .iter()
                    .any(|action| matches!(action, Action::Display { .. })),
                start != target
            );
            if start == target {
                assert_eq!(selected.state, app);
            }
        }
    }
}

#[test]
fn configuration_projection_preserves_content_and_acknowledgements_match_request_identity() {
    use device_core::settings::{
        Configuration, ConfigurationId, ConfigurationRequest, ResumePolicy,
    };
    let base = Settings {
        refresh_seconds: 1800,
        rotation: 2,
        resume: ResumePolicy::Status,
        daily: Daily::default().edit(DailyEdit::Note(Text::new("Keep content").unwrap())),
        selected_task: TaskId::new(6).unwrap(),
        calendar_month: Month::new(2026, 11),
        ..Settings::default()
    };
    for resume in [ResumePolicy::Last, ResumePolicy::Status] {
        let configuration = Configuration {
            refresh_seconds: 600,
            rotation: 3,
            resume,
        };
        let projected = base.with_configuration(configuration, ScreenId::Checklist);
        assert_eq!(projected.refresh_seconds, 600);
        assert_eq!(projected.rotation, 3);
        assert_eq!(projected.resume, resume);
        assert_eq!(
            projected.screen,
            if resume == ResumePolicy::Last {
                ScreenId::Checklist
            } else {
                ScreenId::Status
            }
        );
        assert_eq!(projected.daily, base.daily);
        assert_eq!(projected.selected_task, base.selected_task);
        assert_eq!(projected.calendar_month, base.calendar_month);
        assert_eq!(projected.configuration(), configuration);
        let first = ConfigurationRequest::new(ConfigurationId::new(1), configuration);
        let repeat = ConfigurationRequest::new(ConfigurationId::new(3), configuration);
        assert_eq!(repeat.id(), ConfigurationId::new(3));
        assert_eq!(repeat.value(), configuration);
        assert_eq!(repeat.acknowledge(first.id()), Some(repeat));
        assert_eq!(repeat.acknowledge(repeat.id()), None);
    }
}

#[test]
fn calendar_gestures_follow_the_clock_and_unassigned_holds_do_no_work() {
    use device_core::telemetry::{DateTime, Sample};
    let mut app = App {
        screen: ScreenId::Calendar,
        ..App::default()
    };
    app.telemetry.clock = Sample::Fresh {
        value: DateTime::parse("2026-12-31T23:59:59").unwrap(),
        measured_at_ms: 0,
    };
    assert_eq!(app.middle_label(), "TODAY");
    let browsed = app.transition(AppEvent::Hold(Button::Next)).state;
    assert_eq!(browsed.calendar(), Month::new(2027, 1));
    let previous = browsed.transition(AppEvent::Hold(Button::Previous)).state;
    assert_eq!(previous.calendar(), Month::new(2026, 12));
    let today = browsed.transition(AppEvent::Button(Button::Refresh)).state;
    assert_eq!(today.calendar_month, None);
    assert_eq!(today.calendar(), Month::new(2026, 12));
    assert_eq!(App::default().calendar(), None);
    let mut invalid = app;
    if let Sample::Fresh { mut value, .. } = app.telemetry.clock {
        value.day = 0;
        invalid.telemetry.clock = Sample::Fresh {
            value,
            measured_at_ms: 0,
        };
    }
    assert_eq!(invalid.calendar(), None);
    assert_eq!(
        App {
            screen: ScreenId::Checklist,
            ..App::default()
        }
        .middle_label(),
        "REFRESH"
    );
    let populated = App {
        screen: ScreenId::Checklist,
        daily: Daily::default().edit(DailyEdit::SetTask {
            id: TaskId::default(),
            text: Text::new("Buy milk").unwrap(),
        }),
        ..App::default()
    };
    assert_eq!(populated.middle_label(), "DONE");
    for screen in [
        ScreenId::Status,
        ScreenId::Notes,
        ScreenId::Diagnostics,
        ScreenId::Drawing,
    ] {
        for button in [Button::Previous, Button::Next] {
            let app = App {
                screen,
                ..App::default()
            };
            let ignored = app.transition(AppEvent::Hold(button));
            assert_eq!(ignored.state, app);
            assert_eq!(ignored.actions, [Action::None; 3]);
        }
    }
}

#[test]
fn complete_daily_settings_survive_every_partial_write_and_single_bit_corruption() {
    let mut daily = Daily::default().edit(DailyEdit::Note(
        Text::new(&"n".repeat(NOTE_TEXT_BYTES)).unwrap(),
    ));
    for number in 1..=6 {
        let id = TaskId::new(number).unwrap();
        daily = daily
            .edit(DailyEdit::SetTask {
                id,
                text: Text::new(&"t".repeat(TASK_TEXT_BYTES)).unwrap(),
            })
            .edit(DailyEdit::SetDone {
                id,
                done: number % 2 == 0,
            });
    }
    let settings = Settings {
        daily,
        screen: ScreenId::Checklist,
        selected_task: TaskId::new(6).unwrap(),
        calendar_month: Month::new(2099, 12),
        ..Settings::default()
    };
    let bytes = encode(settings, u32::MAX);
    assert_eq!(decode(&bytes), Some((settings, u32::MAX)));
    for cut in 0..SLOT_BYTES {
        let mut interrupted = [0xff; SLOT_BYTES];
        interrupted[..cut].copy_from_slice(&bytes[..cut]);
        assert_eq!(decode(&interrupted), None, "cut={cut}");
    }
    for index in 0..SLOT_BYTES {
        for bit in 0..8 {
            let mut corrupt = bytes;
            corrupt[index] ^= 1 << bit;
            assert_eq!(decode(&corrupt), None, "index={index} bit={bit}");
        }
    }
}

#[test]
fn a_save_acknowledgement_preserves_more_recent_daily_edits() {
    let id = TaskId::new(1).unwrap();
    let first = App::default()
        .transition(AppEvent::EditDaily(DailyEdit::SetTask {
            id,
            text: Text::new("First edit").unwrap(),
        }))
        .state;
    let saved = first.settings();
    let latest = first
        .transition(AppEvent::EditDaily(DailyEdit::SetTask {
            id,
            text: Text::new("Latest edit").unwrap(),
        }))
        .state;
    let acknowledged = latest.transition(AppEvent::Configure(saved)).state;
    assert_eq!(acknowledged.daily.task(id).text.as_str(), "Latest edit");
    assert_ne!(acknowledged.settings(), acknowledged.persisted_settings);
    let final_saved = acknowledged.settings();
    let committed = acknowledged
        .transition(AppEvent::Configure(final_saved))
        .state;
    assert_eq!(committed.settings(), committed.persisted_settings);
}

#[test]
fn a_save_acknowledgement_preserves_an_edit_reverted_to_the_previous_saved_value() {
    let id = TaskId::default();
    for (edit, revert) in [
        (
            DailyEdit::SetTask {
                id,
                text: Text::new("Buy milk").unwrap(),
            },
            DailyEdit::Clear(id),
        ),
        (
            DailyEdit::Note(Text::new("Library books.").unwrap()),
            DailyEdit::Note(Text::default()),
        ),
    ] {
        let changed = App::default().transition(AppEvent::EditDaily(edit)).state;
        let saving = changed.settings();
        let reverted = changed.transition(AppEvent::EditDaily(revert)).state;
        let acknowledged = reverted.transition(AppEvent::Configure(saving)).state;
        assert_eq!(acknowledged.daily, reverted.daily);
        assert_ne!(acknowledged.settings(), acknowledged.persisted_settings);
    }
}

#[test]
fn held_navigation_keeps_the_page_and_middle_has_one_contextual_action() {
    let id = TaskId::new(1).unwrap();
    let app = App {
        screen: ScreenId::Checklist,
        daily: Daily::default().edit(DailyEdit::SetTask {
            id,
            text: Text::new("Buy milk").unwrap(),
        }),
        ..App::default()
    };
    let completed = app.transition(AppEvent::Button(Button::Refresh));
    assert_eq!(completed.state.daily.counts(), (1, 1));
    assert!(!completed.actions.contains(&Action::Measure));
    assert_eq!(completed.state.middle_label(), "REOPEN");
    let selected = app.transition(AppEvent::Hold(Button::Next)).state;
    assert_eq!(selected.screen, ScreenId::Checklist);
    assert_eq!(selected.selected_task.number(), 2);
    assert!(selected
        .transition(AppEvent::Hold(Button::Refresh))
        .actions
        .contains(&Action::Measure));
    for invalid in [
        "task set 1",
        "task done 1 extra",
        "task unknown 1",
        "task set 0 value",
        "hold unknown",
        "rotation 4",
        "screen unknown",
    ] {
        assert!(commands::parse(invalid).unwrap().is_err(), "{invalid}");
    }
}
