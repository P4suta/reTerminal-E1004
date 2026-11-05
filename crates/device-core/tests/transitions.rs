use device_core::{app::*, telemetry::*};

#[test]
fn panel_failures_revoke_sleep_and_ignore_failures_from_an_old_generation() {
    for sequence in [0, u32::MAX] {
        for refresh in [
            RefreshState::Idle,
            RefreshState::Updating {
                id: sequence,
                started_ms: 0,
            },
            RefreshState::Failed(Fault::InvalidRtc),
        ] {
            for fault in [
                Fault::Bus,
                Fault::PanelTimeout,
                Fault::Storage,
                Fault::SensorCrc,
                Fault::InvalidRtc,
                Fault::InvalidInput,
            ] {
                let app = App {
                    sequence,
                    refresh,
                    sleep_requested: true,
                    ..App::default()
                };
                let failed = app.transition(AppEvent::PanelFailed {
                    id: sequence,
                    fault,
                });
                assert_eq!(failed.state.refresh, RefreshState::Failed(fault));
                assert_eq!(failed.state.diagnostics.last_error, Some(fault));
                assert!(!failed.state.sleep_requested);
                assert_eq!(
                    failed.actions,
                    [Action::StopPanel, Action::Log(fault), Action::None]
                );
                let late = failed.state.transition(AppEvent::DisplayFinished {
                    id: sequence,
                    result: Ok(()),
                });
                assert_eq!(late.state, failed.state);
                let unchanged = App {
                    sequence,
                    refresh,
                    ..App::default()
                };
                let stale = unchanged.transition(AppEvent::PanelFailed {
                    id: sequence.wrapping_add(1),
                    fault,
                });
                assert_eq!(stale.state, unchanged);
                assert_eq!(stale.actions, [Action::None; 3]);
            }
        }
    }
}

#[test]
fn the_next_periodic_refresh_retries_a_failed_panel_and_rejects_old_completions() {
    let started = App::default()
        .transition(AppEvent::Wake(WakeReason::PowerOn))
        .state;
    let failed = started
        .transition(AppEvent::DisplayFinished {
            id: started.sequence,
            result: Err(Fault::PanelTimeout),
        })
        .state;
    let retry = failed.transition(AppEvent::Tick(REFRESH_INTERVAL_MS));
    assert!(retry.actions.contains(&Action::Measure));
    assert!(retry.actions.contains(&Action::Display { id: 2 }));
    let late = retry.state.transition(AppEvent::DisplayFinished {
        id: 1,
        result: Ok(()),
    });
    assert_eq!(late.state.refresh, retry.state.refresh);
    assert_eq!(late.actions, [Action::None; 3]);
}

#[test]
fn diagnostics_redraws_changed_storage_and_radio_without_refreshing_unchanged_values() {
    let mut app = App {
        screen: ScreenId::Diagnostics,
        ..App::default()
    };
    let changed = app.transition(AppEvent::StorageChanged(StorageStatus::Ready));
    assert!(changed
        .actions
        .iter()
        .any(|action| matches!(action, Action::Display { .. })));
    let id = changed.state.sequence;
    app = changed
        .state
        .transition(AppEvent::DisplayFinished { id, result: Ok(()) })
        .state;
    assert!(!app
        .transition(AppEvent::StorageChanged(StorageStatus::Ready))
        .actions
        .iter()
        .any(|action| matches!(action, Action::Display { .. })));
    let changed = app.transition(AppEvent::RadioChanged {
        wifi: RadioStatus::Connected,
        ble: RadioStatus::Off,
    });
    assert!(changed
        .actions
        .iter()
        .any(|action| matches!(action, Action::Display { .. })));
    let app = changed
        .state
        .transition(AppEvent::DisplayFinished {
            id: changed.state.sequence,
            result: Ok(()),
        })
        .state;
    let unchanged = AppEvent::RadioChanged {
        wifi: RadioStatus::Connected,
        ble: RadioStatus::Off,
    };
    assert_eq!(app.transition(unchanged).state.refresh, RefreshState::Idle);
    let ble_changed = AppEvent::RadioChanged {
        wifi: RadioStatus::Connected,
        ble: RadioStatus::Connected,
    };
    assert!(matches!(
        app.transition(ble_changed).state.refresh,
        RefreshState::Updating { .. }
    ));
    let status = App {
        screen: ScreenId::Status,
        ..app
    };
    assert_eq!(
        status.transition(ble_changed).state.refresh,
        RefreshState::Idle
    );
    assert_eq!(
        status
            .transition(AppEvent::StorageChanged(StorageStatus::Fault))
            .state
            .refresh,
        RefreshState::Idle
    );
}

fn step(app: &mut App, event: AppEvent) -> [Action; 3] {
    let transition = app.transition(event);
    *app = transition.state;
    transition.actions
}

#[test]
fn busy_inputs_coalesce_to_one_request_for_the_latest_screen() {
    let mut app = App::default();
    let actions = step(&mut app, AppEvent::Wake(WakeReason::PowerOn));
    assert!(actions.contains(&Action::Measure));
    assert!(actions.contains(&Action::Display { id: 1 }));
    for _ in 0..1000 {
        assert!(!step(&mut app, AppEvent::Button(Button::Next))
            .iter()
            .any(|a| matches!(a, Action::Display { .. })));
    }
    assert_eq!(app.screen, ScreenId::Diagnostics);
    assert!(app.pending);
    assert!(step(
        &mut app,
        AppEvent::DisplayFinished {
            id: 1,
            result: Ok(())
        }
    )
    .contains(&Action::Display { id: 2 }));
    assert!(!app.pending);
    assert_eq!(
        step(
            &mut app,
            AppEvent::DisplayFinished {
                id: 1,
                result: Ok(())
            }
        ),
        [Action::None; 3]
    );
    step(
        &mut app,
        AppEvent::DisplayFinished {
            id: 2,
            result: Ok(()),
        },
    );
    assert_eq!(app.refresh, RefreshState::Idle);
}

#[test]
fn sleep_waits_for_display_and_storage_and_refuses_timeout() {
    let mut app = App::default();
    step(&mut app, AppEvent::Wake(WakeReason::Button));
    step(&mut app, AppEvent::MeasurementBusy(false));
    step(&mut app, AppEvent::StorageBusy(true));
    assert!(!step(&mut app, AppEvent::RequestSleep)
        .iter()
        .any(|a| matches!(a, Action::Sleep { .. })));
    assert!(!step(
        &mut app,
        AppEvent::DisplayFinished {
            id: 1,
            result: Ok(())
        }
    )
    .iter()
    .any(|a| matches!(a, Action::Sleep { .. })));
    assert!(step(&mut app, AppEvent::StorageBusy(false)).contains(&Action::Sleep { seconds: 900 }));
    let mut app = App::default();
    step(&mut app, AppEvent::Wake(WakeReason::Timer));
    step(&mut app, AppEvent::RequestSleep);
    assert!(!step(&mut app, AppEvent::Tick(DISPLAY_TIMEOUT_MS - 1))
        .contains(&Action::Log(Fault::PanelTimeout)));
    let actions = step(&mut app, AppEvent::Tick(DISPLAY_TIMEOUT_MS));
    assert!(actions.contains(&Action::StopPanel));
    assert!(actions.contains(&Action::Log(Fault::PanelTimeout)));
    assert_eq!(app.refresh, RefreshState::Failed(Fault::PanelTimeout));
    assert!(!step(
        &mut app,
        AppEvent::DisplayFinished {
            id: 1,
            result: Ok(())
        }
    )
    .iter()
    .any(|a| matches!(a, Action::Sleep { .. })));
}

#[test]
fn periodic_refresh_staleness_and_clock_rollback_are_explicit() {
    let mut app = App::default();
    assert!(step(&mut app, AppEvent::Tick(REFRESH_INTERVAL_MS - 1))
        .iter()
        .all(|a| *a == Action::None));
    assert!(step(&mut app, AppEvent::Tick(REFRESH_INTERVAL_MS)).contains(&Action::Measure));
    step(&mut app, AppEvent::Tick(0));
    assert_eq!(app.now_ms, REFRESH_INTERVAL_MS);
    let sample = Sample::Fresh {
        value: 42u8,
        measured_at_ms: 100,
    };
    assert!(matches!(sample.at(200, 100), Sample::Fresh { .. }));
    assert!(matches!(sample.at(201, 100), Sample::Stale { .. }));
    assert!(matches!(sample.at(99, 100), Sample::Stale { .. }));
}

#[test]
fn missing_hardware_and_bad_rtc_do_not_become_plausible_measurements() {
    let mut app = App::default();
    step(&mut app, AppEvent::StorageChanged(StorageStatus::Absent));
    assert_eq!(app.diagnostics.storage, StorageStatus::Absent);
    assert_eq!(app.telemetry.clock, Sample::Unavailable);
    let invalid = DateTime {
        year: 2025,
        month: 2,
        day: 29,
        hour: 0,
        minute: 0,
        second: 0,
    };
    assert!(!invalid.valid());
    assert!(DateTime {
        year: 2024,
        ..invalid
    }
    .valid());
    assert!(!DateTime {
        year: 2100,
        ..invalid
    }
    .valid());
}

#[test]
fn measurement_and_configuration_work_finish_before_sleep() {
    let mut app = App::default();
    step(&mut app, AppEvent::MeasurementBusy(true));
    assert!(!step(&mut app, AppEvent::RequestSleep)
        .iter()
        .any(|a| matches!(a, Action::Sleep { .. })));
    step(
        &mut app,
        AppEvent::Configure(device_core::settings::Settings {
            refresh_seconds: 1800,
            rotation: 1,
        }),
    );
    assert_eq!(app.refresh_seconds, 1800);
    assert_eq!(app.rotation, device_core::geometry::Rotation::Landscape);
    let id = match app.refresh {
        RefreshState::Updating { id, .. } => id,
        _ => panic!("configuration must redraw"),
    };
    step(&mut app, AppEvent::DisplayFinished { id, result: Ok(()) });
    assert!(
        step(&mut app, AppEvent::MeasurementBusy(false)).contains(&Action::Sleep { seconds: 1800 })
    );
    let previous = app;
    step(
        &mut app,
        AppEvent::Configure(device_core::settings::Settings {
            refresh_seconds: 0,
            rotation: 255,
        }),
    );
    assert_eq!(app, previous);
}

#[test]
fn every_screen_direction_and_configuration_orientation_is_observable() {
    use device_core::{geometry::Rotation, settings::Settings};
    for (rotation, expected) in [
        (0, Rotation::Portrait),
        (1, Rotation::Landscape),
        (2, Rotation::InvertedPortrait),
        (3, Rotation::InvertedLandscape),
    ] {
        let mut app = App::default();
        step(
            &mut app,
            AppEvent::Configure(Settings {
                refresh_seconds: 1800,
                rotation,
            }),
        );
        assert_eq!(app.rotation, expected);
        assert_eq!(app.next_refresh_ms, 1_800_000);
    }
    for (start, previous, next) in [
        (ScreenId::Status, ScreenId::Drawing, ScreenId::Diagnostics),
        (ScreenId::Diagnostics, ScreenId::Status, ScreenId::Drawing),
        (ScreenId::Drawing, ScreenId::Diagnostics, ScreenId::Status),
    ] {
        for (button, expected) in [
            (Button::Previous, previous),
            (Button::Next, next),
            (Button::Refresh, start),
        ] {
            let mut app = App {
                screen: start,
                ..App::default()
            };
            let actions = step(&mut app, AppEvent::Button(button));
            assert_eq!(app.screen, expected);
            assert_eq!(
                actions.contains(&Action::Measure),
                button == Button::Refresh
            );
        }
    }
    let mut app = App::default();
    step(&mut app, AppEvent::Tick(900_000));
    assert_eq!(app.next_refresh_ms, 1_800_000);
    step(
        &mut app,
        AppEvent::RadioChanged {
            wifi: RadioStatus::Connected,
            ble: RadioStatus::Disconnected,
        },
    );
    assert_eq!(
        (app.diagnostics.wifi, app.diagnostics.ble),
        (RadioStatus::Connected, RadioStatus::Disconnected)
    );
    step(
        &mut app,
        AppEvent::DisplayFinished {
            id: 1,
            result: Err(Fault::Bus),
        },
    );
    assert_eq!(app.refresh, RefreshState::Failed(Fault::Bus));
    let actions = step(&mut app, AppEvent::Button(Button::Refresh));
    assert!(actions.iter().any(|a| matches!(a, Action::Display { .. })));
}
