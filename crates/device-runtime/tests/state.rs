use core::{
    future::Future,
    sync::atomic::Ordering,
    task::{Context, Poll, Waker},
};
use device_core::{app::AppEvent, telemetry::RadioStatus};
use device_runtime::{policy::Radios, state::*};

fn complete(future: impl Future<Output = ()>) {
    assert!(matches!(
        core::pin::pin!(future)
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop())),
        Poll::Ready(())
    ));
}
#[test]
fn active_controllers_and_pending_commands_each_independently_block_sleep() {
    for active in 0..4 {
        for pending in 0..4 {
            let system = System::new(Radios {
                wifi: true,
                ble: true,
            });
            system.radio_active.store(active, Ordering::Release);
            if pending & 1 != 0 {
                system.wifi_command.try_send(WifiCommand::Scan).unwrap();
            }
            if pending & 2 != 0 {
                system.ble_enabled.signal(false);
            }
            assert_eq!(system.radios_quiet(), active == 0 && pending == 0);
        }
    }
    let system = System::new(Radios {
        wifi: true,
        ble: true,
    });
    for radio in [WIFI_ACTIVE, BLE_ACTIVE] {
        system.active(radio, true);
        system.active(radio, true);
        assert_eq!(system.radio_active.load(Ordering::Acquire), radio);
        assert!(system.radio_changed.signaled());
        system.active(radio, false);
        system.active(radio, false);
        assert!(system.radios_quiet());
    }
}
#[test]
fn independently_updated_radio_states_never_overwrite_the_other_controller() {
    let system = System::new(Radios {
        wifi: true,
        ble: true,
    });
    complete(system.radio_status(Some(RadioStatus::Connected), None));
    assert_eq!(
        system.events.try_receive().unwrap(),
        AppEvent::RadioChanged {
            wifi: RadioStatus::Connected,
            ble: RadioStatus::Off
        }
    );
    complete(system.radio_status(None, Some(RadioStatus::Disconnected)));
    assert_eq!(
        system.events.try_receive().unwrap(),
        AppEvent::RadioChanged {
            wifi: RadioStatus::Connected,
            ble: RadioStatus::Disconnected
        }
    );
    complete(system.radio_status(Some(RadioStatus::Off), None));
    assert_eq!(
        system.events.try_receive().unwrap(),
        AppEvent::RadioChanged {
            wifi: RadioStatus::Off,
            ble: RadioStatus::Disconnected
        }
    );
}

#[test]
fn only_a_successful_current_transfer_publishes_its_snapshot() {
    use device_core::{app::App, telemetry::Fault};
    let system = System::new(Radios {
        wifi: false,
        ble: false,
    });
    let original = App::default();
    assert_eq!(system.presented(), None);
    system.start_display(1, original);
    system.finish_display(2, Ok(()));
    assert_eq!(system.presented(), None);
    system.finish_display(1, Ok(()));
    assert_eq!(system.presented(), Some(original));
    let changed = App {
        sequence: 2,
        ..original
    };
    for cancel in [true, false] {
        system.start_display(2, changed);
        if cancel {
            system.cancel_display();
            system.finish_display(2, Ok(()));
        } else {
            system.finish_display(2, Err(Fault::Bus));
        }
        system.finish_display(2, Ok(()));
        assert_eq!(system.presented(), Some(original));
    }
    system.start_display(3, changed);
    system.finish_display(3, Ok(()));
    assert_eq!(system.presented(), Some(changed));
}
