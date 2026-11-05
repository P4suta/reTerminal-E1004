use crate::{measurements::charging_status, policy::*};
use device_core::{app::*, settings::*, telemetry::*};

#[kani::proof]
#[kani::unwind(6)]
fn sleep_permission_requires_current_ticket_and_every_quiet_boundary() {
    let mut app = App::default();
    app.sequence = kani::any();
    app.refresh_seconds = kani::any();
    app.pending = kani::any();
    app.storage_busy = kani::any();
    app.measurement_busy = kani::any();
    app.sleep_requested = kani::any();
    app.refresh = match kani::any::<u8>() % 3 {
        0 => RefreshState::Idle,
        1 => RefreshState::Failed(Fault::Bus),
        _ => RefreshState::Updating {
            id: kani::any(),
            started_ms: kani::any(),
        },
    };
    let ticket = SleepTicket {
        sequence: kani::any(),
        seconds: kani::any(),
    };
    let boundaries: [bool; 5] = kani::any();
    let sleep_pending: bool = kani::any();
    let condition = SleepConditions {
        app,
        ticket,
        buttons_armed: boundaries[0],
        outputs_quiet: boundaries[1],
        radios_quiet: boundaries[2],
        bus_empty: boundaries[3],
        sleep_pending,
        events_empty: boundaries[4],
    };
    let work_idle = app.refresh == RefreshState::Idle
        && !(app.pending || app.storage_busy || app.measurement_busy || app.sleep_requested);
    let expected = work_idle
        && app.sequence == ticket.sequence
        && app.refresh_seconds == ticket.seconds
        && boundaries.into_iter().all(|quiet| quiet)
        && !sleep_pending;
    assert_eq!(condition.permitted(), expected);
    kani::cover!(condition.permitted());
    kani::cover!(!condition.permitted() && work_idle);
}

#[kani::proof]
#[kani::unwind(5)]
fn an_emitted_sleep_action_passes_the_request_boundary() {
    let mut app = App::default();
    app.sequence = kani::any();
    let transition = app.transition(AppEvent::RequestSleep);
    for action in transition.actions {
        if let Action::Sleep { seconds } = action {
            assert!(SleepTicket {
                sequence: transition.state.sequence,
                seconds
            }
            .current(transition.state));
        }
    }
    assert!(transition
        .actions
        .iter()
        .any(|action| matches!(action, Action::Sleep { .. })));
}

#[kani::proof]
#[kani::unwind(2)]
fn display_requests_match_only_the_active_generation() {
    let id: u32 = kani::any();
    let requested: u32 = kani::any();
    let mut app = App::default();
    app.refresh = match kani::any::<u8>() % 3 {
        0 => RefreshState::Idle,
        1 => RefreshState::Failed(Fault::Bus),
        _ => RefreshState::Updating {
            id,
            started_ms: kani::any(),
        },
    };
    assert_eq!(
        display_is_current(app, requested),
        matches!(app.refresh, RefreshState::Updating { .. }) && id == requested
    );
}

#[kani::proof]
#[kani::unwind(50)]
fn erased_settings_are_defaults_and_non_erased_corruption_is_reported() {
    let mut first = [0xff; SLOT_BYTES];
    let second = [0xff; SLOT_BYTES];
    let index: usize = kani::any();
    kani::assume(index < SLOT_BYTES);
    let byte: u8 = kani::any();
    first[index] = byte;
    let (settings, error) = loaded_settings(&first, &second);
    assert_eq!(settings, Settings::default());
    assert_eq!(
        error,
        if byte == 0xff {
            None
        } else {
            Some(Fault::Storage)
        }
    );
}

fn boot_slot_failures<const FAILURES: u8>() {
    use e1004_bsp::storage::{SlotStorage, StorageError};
    struct Flash<const FAILURES: u8> {
        error: StorageError,
        reads: usize,
        wrote: bool,
    }
    impl<const FAILURES: u8> SlotStorage for Flash<FAILURES> {
        fn read(&mut self, slot: usize, bytes: &mut [u8; SLOT_BYTES]) -> Result<(), StorageError> {
            self.reads += 1;
            let failed = FAILURES & (1 << slot) != 0;
            let value = if failed {
                Settings {
                    refresh_seconds: 86400,
                    rotation: 2,
                }
            } else if slot == 0 {
                Settings {
                    refresh_seconds: 600,
                    rotation: 1,
                }
            } else {
                Settings {
                    refresh_seconds: 1800,
                    rotation: 3,
                }
            };
            *bytes = encode(value, if failed { 500 } else { slot as u32 });
            if failed {
                Err(self.error)
            } else {
                Ok(())
            }
        }
        fn write(&mut self, _: usize, _: usize, _: &[u8]) -> Result<(), StorageError> {
            self.wrote = true;
            Ok(())
        }
        fn sync(&mut self) -> Result<(), StorageError> {
            self.wrote = true;
            Ok(())
        }
    }
    let error = match kani::any::<u8>() % 3 {
        0 => StorageError::Absent,
        1 => StorageError::Io,
        _ => StorageError::InvalidSettings,
    };
    let mut flash = Flash::<FAILURES> {
        error,
        reads: 0,
        wrote: false,
    };
    let expected = match FAILURES {
        0 | 1 => Settings {
            refresh_seconds: 1800,
            rotation: 3,
        },
        2 => Settings {
            refresh_seconds: 600,
            rotation: 1,
        },
        _ => Settings::default(),
    };
    assert_eq!(
        crate::tasks::boot_settings(&mut flash),
        (
            expected,
            if FAILURES == 0 {
                None
            } else {
                Some(Fault::Storage)
            }
        )
    );
    assert_eq!(flash.reads, 2);
    assert!(!flash.wrote);
}

#[kani::proof]
#[kani::unwind(50)]
fn boot_recovers_with_both_slots_readable() {
    boot_slot_failures::<0>();
}

#[kani::proof]
#[kani::unwind(50)]
fn boot_recovers_with_first_slot_unreadable() {
    boot_slot_failures::<1>();
}

#[kani::proof]
#[kani::unwind(50)]
fn boot_recovers_with_second_slot_unreadable() {
    boot_slot_failures::<2>();
}

#[kani::proof]
#[kani::unwind(50)]
fn boot_reports_failure_with_both_slots_unreadable() {
    boot_slot_failures::<3>();
}

#[kani::proof]
#[kani::unwind(2)]
fn charger_status_reads_only_the_two_documented_bits() {
    let register: u8 = kani::any();
    let state = (register / 8) % 4;
    assert_eq!(
        charging_status(register),
        if state == 1 || state == 2 {
            Charging::Charging
        } else {
            Charging::NotCharging
        }
    );
}

#[kani::proof]
fn packet_reservations_never_exceed_capacity_or_release_an_invalid_count() {
    use crate::packet_pool::{release, reserve, PACKET_CAPACITY};
    let used: u8 = kani::any();
    match reserve(used) {
        Some(next) => {
            assert!(used < PACKET_CAPACITY);
            assert_eq!(next, used + 1);
            assert!(next <= PACKET_CAPACITY);
            assert_eq!(release(next), Some(used));
        }
        None => assert!(used >= PACKET_CAPACITY),
    }
    match release(used) {
        Some(next) => {
            assert!(used > 0 && used <= PACKET_CAPACITY);
            assert_eq!(next + 1, used);
            assert!(next < PACKET_CAPACITY);
        }
        None => assert!(used == 0 || used > PACKET_CAPACITY),
    }
}
