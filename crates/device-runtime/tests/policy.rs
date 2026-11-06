use device_core::{app::*, settings::Settings};
use device_runtime::policy::*;

#[test]
fn every_panel_error_retains_its_fault_category() {
    use device_core::telemetry::Fault;
    for (error, expected) in [
        (t133a01::Error::Transport, Fault::Bus),
        (t133a01::Error::BusyTimeout, Fault::PanelTimeout),
        (t133a01::Error::InvalidFrame, Fault::Bus),
        (t133a01::Error::NotInitialized, Fault::Bus),
    ] {
        assert_eq!(panel_fault(error), expected);
    }
}

#[test]
fn a_sleep_ticket_never_allows_an_active_radio_or_a_stale_request() {
    let transition = App::default().transition(AppEvent::RequestSleep);
    assert!(transition
        .actions
        .iter()
        .any(|action| matches!(action, Action::Sleep { .. })));
    let app = transition.state;
    let mut conditions = SleepConditions {
        app,
        ticket: SleepTicket {
            sequence: 0,
            seconds: 900,
        },
        buttons_armed: true,
        outputs_quiet: true,
        radios_quiet: true,
        bus_empty: true,
        sleep_pending: false,
        events_empty: true,
    };
    assert!(conditions.permitted());
    conditions.radios_quiet = false;
    assert!(
        !conditions.permitted(),
        "radio controllers must release their hardware before sleep"
    );
    conditions.radios_quiet = true;
    conditions.ticket.sequence = 1;
    assert!(!conditions.permitted());
    conditions.ticket.sequence = 0;
    conditions.ticket.seconds = Settings::default().refresh_seconds + 1;
    assert!(!conditions.permitted());
}

#[test]
fn every_busy_boundary_and_every_stale_ticket_blocks_sleep() {
    let app = App::default();
    for mask in 0u16..2048 {
        let mut condition = SleepConditions {
            app,
            ticket: SleepTicket {
                sequence: 0,
                seconds: 900,
            },
            buttons_armed: mask & 1 == 0,
            outputs_quiet: mask & 2 == 0,
            radios_quiet: mask & 4 == 0,
            bus_empty: mask & 8 == 0,
            sleep_pending: mask & 1024 != 0,
            events_empty: mask & 16 == 0,
        };
        condition.app.pending = mask & 32 != 0;
        condition.app.storage_busy = mask & 64 != 0;
        condition.app.measurement_busy = mask & 128 != 0;
        condition.app.sleep_requested = mask & 256 != 0;
        if mask & 512 != 0 {
            condition.app.refresh = RefreshState::Failed(device_core::telemetry::Fault::Bus);
        }
        assert_eq!(condition.permitted(), mask == 0, "mask={mask}");
    }
    for state in [
        RefreshState::Idle,
        RefreshState::Failed(device_core::telemetry::Fault::Bus),
        RefreshState::Updating {
            id: 5,
            started_ms: 0,
        },
    ] {
        let app = App {
            refresh: state,
            ..app
        };
        assert_eq!(
            display_is_current(app, 5),
            matches!(state, RefreshState::Updating { .. })
        );
        assert!(!display_is_current(app, 6));
    }
}

#[test]
fn settings_boot_distinguishes_erased_corrupt_and_valid_generations() {
    use device_core::{settings::*, telemetry::Fault};
    let erased = [0xff; SLOT_BYTES];
    assert_eq!(
        loaded_settings(&erased, &erased),
        (Settings::default(), None)
    );
    assert_eq!(
        loaded_settings(&[0; SLOT_BYTES], &erased),
        (Settings::default(), Some(Fault::SettingsStorage))
    );
    let expected = Settings {
        refresh_seconds: 1800,
        rotation: 3,
        ..Settings::default()
    };
    let valid = encode(expected, 4);
    assert_eq!(loaded_settings(&valid, &erased), (expected, None));
    assert_eq!(loaded_settings(&erased, &valid), (expected, None));
    for index in 0..SLOT_BYTES {
        let mut corrupt = erased;
        corrupt[index] = 0;
        assert_eq!(
            loaded_settings(&corrupt, &erased),
            (Settings::default(), Some(Fault::SettingsStorage))
        );
    }
}
