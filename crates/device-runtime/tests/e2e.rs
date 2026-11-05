mod support;
use device_core::{
    app::*,
    geometry::{FRAME_BYTES, PACKED_WHITE},
    telemetry::*,
};
use device_runtime::{
    buttons::buttons_loop, console::console_loop, policy::Radios, state::System, tasks::*,
};
use support::*;

#[test]
fn virtual_time_advances_while_a_ready_task_keeps_yielding() {
    let _clock = clock();
    let ticks = core::cell::Cell::new(0);
    let woke = core::cell::Cell::new(false);
    let untouched = core::cell::Cell::new(0);
    let mut executor = Executor::new();
    executor.spawn(async {
        loop {
            ticks.set(ticks.get() + 1);
            embassy_futures::yield_now().await;
        }
    });
    executor.spawn(async {
        embassy_time::Timer::after_millis(3).await;
        woke.set(true);
        core::future::pending::<()>().await;
    });
    executor.spawn(async {
        untouched.set(untouched.get() + 1);
        core::future::pending::<()>().await;
    });
    assert!(executor.until(4, || woke.get()));
    assert!(ticks.get() > 0);
    assert_eq!(untouched.get(), 1);
    assert_eq!(embassy_time::Instant::now().as_millis(), 3);
}

#[test]
fn physical_active_low_buttons_emit_one_event_only_after_a_stable_press() {
    let _clock = clock();
    let system = System::new(Radios {
        wifi: false,
        ble: false,
    });
    let board = Board::new();
    let mut executor = Executor::new();
    executor.spawn(buttons_loop(&system, board.clone()));
    executor.advance(100);
    assert!(system.events.is_empty());
    for (index, button) in [Button::Previous, Button::Refresh, Button::Next]
        .into_iter()
        .enumerate()
    {
        for high in [false, true, false, true] {
            board.0.borrow_mut().buttons[index] = high;
            executor.advance(20);
            assert!(system.events.is_empty());
        }
        board.0.borrow_mut().buttons[index] = false;
        executor.advance(40);
        assert!(system.events.is_empty());
        executor.advance(20);
        assert_eq!(system.events.try_receive(), Ok(AppEvent::Button(button)));
        executor.advance(200);
        assert!(system.events.is_empty());
        board.0.borrow_mut().buttons[index] = true;
        executor.advance(80);
        assert!(system.events.is_empty());
        board.0.borrow_mut().buttons[index] = false;
        executor.advance(60);
        assert_eq!(system.events.try_receive(), Ok(AppEvent::Button(button)));
        assert!(system.events.is_empty());
        board.0.borrow_mut().buttons[index] = true;
        executor.advance(80);
    }
}

#[test]
fn real_firmware_tasks_boot_measure_serve_console_persist_and_sleep_then_reboot() {
    let _clock = clock();
    let mut board = Board::new();
    board.0.borrow_mut().buttons[1] = false;
    let (settings, error) = loaded(&mut board);
    let system = System::new(Radios {
        wifi: false,
        ble: false,
    });
    let mut frame = vec![PACKED_WHITE; FRAME_BYTES];
    let mut executor = Executor::new();
    executor.spawn(application_loop(
        &system,
        settings,
        error,
        WakeReason::PowerOn,
        || (12345, 654321),
    ));
    executor.spawn(bus_loop(&system, board.clone(), board.clone(), &mut frame));
    executor.spawn(measurements(&system, board.clone(), |_| {}));
    executor.spawn(buttons_loop(&system, board.clone()));
    executor.spawn(console_loop(&system, board.clone(), "fixture"));
    executor.spawn(ticks(&system));
    assert!(executor.until(1000, || {
        !board.0.borrow().frames.is_empty()
            && system
                .snapshot()
                .is_some_and(|app| app.refresh == RefreshState::Idle)
    }));
    board.command("status");
    board.command("wifi scan");
    board.command("ble on");
    board.command("interval 1800");
    assert!(executor.until(2000, || {
        system
            .snapshot()
            .is_some_and(|app| app.refresh_seconds == 1800)
    }));
    assert!(board.output().contains("event=status"));
    assert!(board.output().contains("event=telemetry value="));
    assert!(!board.output().contains("event=command_rejected"));
    assert!(board.output().contains("feature_unavailable feature=wifi"));
    assert!(board.output().contains("feature_unavailable feature=ble"));
    assert!(board.0.borrow().sleeping.is_none());
    board.0.borrow_mut().buttons[1] = true;
    assert!(
        executor.until(2000, || board.0.borrow().sleeping.is_some()),
        "sleep failed: app={:?}, trace={:?}, uart={}",
        system.snapshot(),
        board.0.borrow().trace,
        board.output()
    );
    assert_eq!(board.0.borrow().sleeping, Some(1800));
    let app = system.snapshot().unwrap();
    assert!(matches!(app.telemetry.temperature,Sample::Fresh { value,.. } if value.get()==4250));
    assert_eq!(app.diagnostics.heap_bytes, 12345);
    assert_eq!(app.diagnostics.psram_bytes, 654321);
    drop(executor);
    let (settings, error) = loaded(&mut board);
    assert_eq!(settings.refresh_seconds, 1800);
    assert_eq!(error, None);
    board.0.borrow_mut().sleeping = None;
    let reboot = System::new(Radios {
        wifi: false,
        ble: false,
    });
    let mut executor = Executor::new();
    executor.spawn(application_loop(
        &reboot,
        settings,
        error,
        WakeReason::Timer,
        || (12345, 654321),
    ));
    executor.spawn(bus_loop(&reboot, board.clone(), board.clone(), &mut frame));
    executor.spawn(measurements(&reboot, board.clone(), |_| {}));
    executor.spawn(buttons_loop(&reboot, board.clone()));
    executor.spawn(console_loop(&reboot, board.clone(), "fixture"));
    executor.spawn(ticks(&reboot));
    assert!(executor.until(2000, || board.0.borrow().sleeping.is_some()));
    assert_eq!(
        reboot.snapshot().unwrap().diagnostics.wake,
        WakeReason::Timer
    );
    assert_eq!(reboot.snapshot().unwrap().refresh_seconds, 1800);
    assert_eq!(board.output().matches("event=console_ready").count(), 2);
}

macro_rules! boot {
    ($board:ident, $system:ident, $frame:ident, $executor:ident, $configure:expr) => {
        boot!(
            $board,
            $system,
            $frame,
            $executor,
            $configure,
            Radios {
                wifi: false,
                ble: false
            }
        );
    };
    ($board:ident, $system:ident, $frame:ident, $executor:ident, $configure:expr, $capabilities:expr) => {
        boot!(
            $board,
            $system,
            $frame,
            $executor,
            $configure,
            $capabilities,
            WakeReason::PowerOn
        );
    };
    ($board:ident, $system:ident, $frame:ident, $executor:ident, $configure:expr, $capabilities:expr,$wake:expr) => {
        let mut $board = Board::new();
        $configure(&$board);
        let (settings, error) = loaded(&mut $board);
        let $system = System::new($capabilities);
        let mut $frame = vec![PACKED_WHITE; FRAME_BYTES];
        let mut $executor = Executor::new();
        $executor.spawn(application_loop(&$system, settings, error, $wake, || {
            (12345, 654321)
        }));
        $executor.spawn(bus_loop(
            &$system,
            $board.clone(),
            $board.clone(),
            &mut $frame,
        ));
        $executor.spawn(measurements(&$system, $board.clone(), |_| {}));
        $executor.spawn(buttons_loop(&$system, $board.clone()));
        $executor.spawn(console_loop(&$system, $board.clone(), "fixture"));
        $executor.spawn(ticks(&$system));
    };
}

#[cfg(feature = "ble")]
#[test]
fn every_radio_combination_rotation_and_screen_survives_boot_input_sleep_and_wake() {
    let _clock = clock();
    for mode in 0..4 {
        for rotation in 0..4 {
            for screen in 0..3 {
                embassy_time::MockDriver::get().reset();
                let capabilities = Radios {
                    wifi: mode & 1 != 0,
                    ble: mode & 2 != 0,
                };
                let wake = [WakeReason::PowerOn, WakeReason::Timer, WakeReason::Button][screen];
                let interval = [60, 1800, 3600, 86400][usize::from(rotation)];
                boot!(
                    board,
                    system,
                    frame,
                    executor,
                    |board: &Board| {
                        let mut device = board.0.borrow_mut();
                        device.buttons[1] = false;
                        device.slots[0] = device_core::settings::encode(
                            device_core::settings::Settings {
                                refresh_seconds: interval,
                                rotation,
                            },
                            1,
                        );
                    },
                    capabilities,
                    wake
                );
                let wifi = wifi::Factory::default();
                let hci = hci::Hci::default();
                if capabilities.wifi {
                    executor.spawn(device_runtime::wifi::wifi_loop(&system, wifi.clone()));
                }
                if capabilities.ble {
                    executor.spawn(device_runtime::ble::ble_loop(
                        &system,
                        hci::Factory(hci.clone()),
                    ));
                }
                assert!(executor.until(1000, || system
                    .snapshot()
                    .is_some_and(|app| app.refresh == RefreshState::Idle)));
                let initial = system.snapshot().unwrap();
                assert_eq!(initial.rotation as u8, rotation);
                assert_eq!(initial.refresh_seconds, interval);
                assert_eq!(initial.next_refresh_ms, u64::from(interval) * 1000);
                if capabilities.wifi {
                    board.command("wifi scan");
                }
                if capabilities.ble {
                    board.command("ble on");
                }
                executor.advance(50);
                for _ in 0..screen {
                    system
                        .events
                        .try_send(AppEvent::Button(Button::Next))
                        .unwrap();
                }
                executor.advance(400);
                assert_eq!(system.snapshot().unwrap().diagnostics.wake, wake);
                if capabilities.ble {
                    assert!(hci.0.borrow().advertising);
                }
                board.0.borrow_mut().buttons[1] = true;
                assert!(
                    executor.until(3000, || board.0.borrow().sleeping.is_some()),
                    "mode={mode}, rotation={rotation}, screen={screen}, output={}",
                    board.output()
                );
                assert!(system.radios_quiet());
                assert_eq!(wifi.0.borrow().opened, wifi.0.borrow().closed);
                assert_eq!(hci.0.borrow().opens, hci.0.borrow().closes);
                let app = system.presented().unwrap();
                let mut expected = vec![PACKED_WHITE; FRAME_BYTES];
                device_ui::render(
                    &app,
                    &mut device_ui::Framebuffer::new(&mut expected, app.rotation).unwrap(),
                );
                let actual = board.0.borrow().frames.last().unwrap().clone();
                assert!(actual==expected,"wire framebuffer differs at {:?}: mode={mode}, rotation={rotation}, screen={screen}",actual.iter().zip(&expected).position(|(left,right)|left!=right));
                drop(executor);
                assert_eq!(loaded(&mut board).0.rotation, rotation);
            }
        }
    }
}

#[test]
fn wifi_scan_connect_disconnect_and_sleep_release_the_real_task_controller() {
    let _clock = clock();
    boot!(
        board,
        system,
        frame,
        executor,
        |board: &Board| {
            board.0.borrow_mut().buttons[1] = false;
        },
        Radios {
            wifi: true,
            ble: false
        }
    );
    let wifi = wifi::Factory::default();
    executor.spawn(device_runtime::wifi::wifi_loop(&system, wifi.clone()));
    assert!(executor.until(1000, || system
        .snapshot()
        .is_some_and(|app| app.refresh == RefreshState::Idle)));
    board.command("wifi scan");
    board.command("wifi connect fixture\tprivate-key");
    assert!(
        executor.until(1000, || system.snapshot().unwrap().diagnostics.wifi
            == RadioStatus::Connected)
    );
    assert_eq!(wifi.0.borrow().scans, 1);
    assert!(board.output().contains("wifi_scan networks=1"));
    assert!(board.output().contains("signal_dbm=-42"));
    board.command("wifi disconnect");
    assert!(executor.until(1000, || wifi.0.borrow().disconnects == 1));
    executor.advance(20);
    assert_eq!(
        system.snapshot().unwrap().diagnostics.wifi,
        RadioStatus::Disconnected
    );
    assert!(!board.output().contains("private-key"));
    board.0.borrow_mut().buttons[1] = true;
    assert!(executor.until(1000, || board.0.borrow().sleeping.is_some()));
    assert_eq!(wifi.0.borrow().closed, wifi.0.borrow().opened);
    assert!(system.radios_quiet());
}

#[test]
fn radio_operation_timeouts_preserve_console_responsiveness_and_allow_shutdown() {
    let _clock = clock();
    boot!(
        board,
        system,
        frame,
        executor,
        |board: &Board| {
            board.0.borrow_mut().buttons[1] = false;
        },
        Radios {
            wifi: true,
            ble: false
        }
    );
    let wifi = wifi::Factory::default();
    executor.spawn(device_runtime::wifi::wifi_loop(&system, wifi.clone()));
    executor.advance(300);
    wifi.0.borrow_mut().stall_scan = true;
    board.command("wifi scan");
    board.command("status");
    executor.advance(15100);
    assert!(board.output().contains("wifi_scan_failed"));
    assert!(board.output().contains("event=status"));
    wifi.0.borrow_mut().stall_connect = true;
    board.command("wifi connect fixture\tprivate-key");
    executor.advance(20100);
    assert!(board.output().contains("wifi_connect_failed"));
    wifi.0.borrow_mut().stall_disconnect = true;
    board.command("wifi disconnect");
    executor.advance(20100);
    assert!(board.output().contains("wifi_disconnect_failed"));
    board.0.borrow_mut().buttons[1] = true;
    assert!(executor.until(1000, || board.0.borrow().sleeping.is_some()));
    assert!(system.radios_quiet());
}

#[test]
fn events_arriving_while_the_panel_enters_sleep_cancel_deep_sleep() {
    let _clock = clock();
    boot!(board, system, frame, executor, |_: &Board| {});
    assert!(executor.until(1000, || board
        .0
        .borrow()
        .trace
        .iter()
        .any(|event| event.starts_with("panel Left 07"))));
    system
        .events
        .try_send(AppEvent::Button(Button::Next))
        .unwrap();
    executor.advance(10);
    assert!(board.0.borrow().sleeping.is_none());
    assert!(executor.until(1000, || board.0.borrow().sleeping.is_some()));
    assert_eq!(system.snapshot().unwrap().screen, ScreenId::Diagnostics);
}

#[test]
fn panel_sleep_failure_is_reported_and_never_enters_deep_sleep() {
    let _clock = clock();
    boot!(board, system, frame, executor, |board: &Board| {
        board.0.borrow_mut().sleep_failure = true;
    });
    executor.advance(3000);
    assert_eq!(
        system.snapshot().unwrap().diagnostics.last_error,
        Some(Fault::Bus)
    );
    assert_eq!(
        system.snapshot().unwrap().refresh,
        RefreshState::Failed(Fault::Bus)
    );
    assert!(board.0.borrow().sleeping.is_none());
    assert!(!board.0.borrow().panel_on);
}

#[test]
fn failed_wake_configuration_is_rolled_back_then_recovers_without_restart() {
    let _clock = clock();
    boot!(board, system, frame, executor, |board: &Board| {
        board.0.borrow_mut().arm_failure = Some(1);
    });
    executor.advance(1000);
    assert!(board.0.borrow().sleeping.is_none());
    assert_eq!(board.0.borrow().armed, [false; 3]);
    assert!(board.output().contains("button_wake_configuration_failed"));
    board.0.borrow_mut().arm_failure = None;
    assert!(executor.until(1000, || board.0.borrow().sleeping.is_some()));
}

#[test]
fn failed_console_flush_blocks_sleep_and_console_resumes_for_repair() {
    let _clock = clock();
    boot!(board, system, frame, executor, |board: &Board| {
        board.0.borrow_mut().flush_failure = true;
    });
    assert!(executor.until(3000, || board.output().contains("console_flush_failed")));
    assert!(board.0.borrow().sleeping.is_none());
    assert!(board.output().contains("console_flush_failed"));
    board.0.borrow_mut().flush_failure = false;
    assert!(executor.until(3000, || board.0.borrow().sleeping.is_some()));
}

#[test]
fn permanently_busy_panel_times_out_cleans_up_and_manual_refresh_recovers() {
    let _clock = clock();
    boot!(board, system, frame, executor, |board: &Board| {
        board.0.borrow_mut().busy_forever = true;
    });
    executor.advance(120001);
    assert_eq!(
        system.snapshot().unwrap().refresh,
        RefreshState::Failed(Fault::PanelTimeout)
    );
    assert!(!board.0.borrow().panel_on);
    assert!(board.0.borrow().sleeping.is_none());
    board.0.borrow_mut().busy_forever = false;
    board.command("refresh");
    assert!(executor.until(2000, || board.0.borrow().sleeping.is_some()));
}

#[test]
fn a_rejected_hardware_sleep_rearms_and_retries() {
    let _clock = clock();
    boot!(board, system, frame, executor, |board: &Board| {
        board.0.borrow_mut().reject_sleep = true;
    });
    executor.advance(1000);
    assert!(board
        .0
        .borrow()
        .trace
        .iter()
        .any(|event| event.starts_with("deep sleep")));
    assert!(!system.sleeping.load(std::sync::atomic::Ordering::Acquire));
    board.0.borrow_mut().reject_sleep = false;
    assert!(executor.until(1000, || board.0.borrow().sleeping.is_some()));
}

#[test]
fn a_stalled_adc_and_missing_sensor_still_leave_the_console_and_sleep_operational() {
    let _clock = clock();
    boot!(board, system, frame, executor, |board: &Board| {
        let mut device = board.0.borrow_mut();
        device.sensor_failure = Some(device_runtime::measurements::IoFailure::Absent);
        device.adc_pending = true;
        device.charger_failure = true;
        device.rtc[0] = 0x80;
    });
    assert!(executor.until(2000, || board.0.borrow().sleeping.is_some()));
    let app = system.snapshot().unwrap();
    assert_eq!(app.telemetry.temperature, Sample::Missing);
    assert!(matches!(app.telemetry.clock, Sample::Fault(_)));
    assert_eq!(app.telemetry.battery, Sample::Fault(Fault::Bus));
    assert_eq!(app.telemetry.charging, Charging::Unknown);
    assert!(!board.0.borrow().divider);
}

#[cfg(feature = "ble")]
#[test]
fn real_ble_host_advertises_serves_att_battery_reads_notifies_and_releases_the_controller() {
    eprintln!("ble-stage=protocol-entered");
    let _clock = clock();
    let system = System::new(Radios {
        wifi: false,
        ble: true,
    });
    let hci = hci::Hci::default();
    let mut executor = Executor::new();
    executor.spawn(device_runtime::ble::ble_loop(
        &system,
        hci::Factory(hci.clone()),
    ));
    system.battery.signal(73);
    system.ble_enabled.signal(true);
    assert!(
        executor.until(100, || hci.0.borrow().advertising),
        "commands: {:?}",
        hci.0.borrow().commands
    );
    eprintln!("ble-stage=protocol-advertising");
    for byte in 0u8..=255 {
        assert_eq!(
            device_runtime::ble::static_address([1, 2, 3, 4, 5, byte]),
            [1, 2, 3, 4, 5, byte | 0xc0]
        );
    }
    let random = hci
        .0
        .borrow()
        .commands
        .iter()
        .find(|packet| packet[..2] == [5, 0x20])
        .unwrap()
        .clone();
    assert_eq!(&random[3..], &[1, 2, 3, 4, 5, 0xc6]);
    let advertising = hci
        .0
        .borrow()
        .commands
        .iter()
        .find(|packet| packet[..2] == [8, 0x20])
        .unwrap()
        .clone();
    assert_eq!(
        &advertising[4..17],
        &[2, 1, 6, 3, 3, 0x0f, 0x18, 6, 9, b'E', b'1', b'0', b'0']
    );
    hci.connect();
    assert!(executor.until(100, || system.events.len() >= 2));
    hci.att(&[0x08, 1, 0, 0xff, 0xff, 0x19, 0x2a]);
    assert!(
        executor.until(100, || !hci.0.borrow().acl.is_empty()),
        "no ATT reply"
    );
    let response = hci.0.borrow().acl.last().unwrap().clone();
    assert_eq!(&response[8..10], &[0x09, 3]);
    let handle = [response[10], response[11]];
    assert_eq!(response[12], 73);
    eprintln!("ble-stage=protocol-read");
    hci.0.borrow_mut().acl.clear();
    hci.att(&[0x12, handle[0] + 1, handle[1], 1, 0]);
    assert!(executor.until(100, || !hci.0.borrow().acl.is_empty()));
    assert_eq!(hci.0.borrow().acl.last().unwrap()[8], 0x13);
    hci.0.borrow_mut().acl.clear();
    system.battery.signal(54);
    assert!(executor.until(100, || !hci.0.borrow().acl.is_empty()));
    assert_eq!(
        &hci.0.borrow().acl.last().unwrap()[8..],
        &[0x1b, handle[0], handle[1], 54]
    );
    hci.disconnect();
    assert!(
        executor.until(100, || hci.0.borrow().advertising),
        "a disconnected central must release its connection and restart advertising"
    );
    hci.connect();
    hci.0.borrow_mut().acl.clear();
    hci.att(&[0x0a, 0xff, 0xff]);
    assert!(executor.until(100, || !hci.0.borrow().acl.is_empty()));
    assert_eq!(
        &hci.0.borrow().acl.last().unwrap()[8..],
        &[1, 0x0a, 0xff, 0xff, 1]
    );
    hci.0.borrow_mut().acl.clear();
    hci.att(&[0x0a, handle[0], handle[1]]);
    assert!(executor.until(100, || !hci.0.borrow().acl.is_empty()));
    assert_eq!(&hci.0.borrow().acl.last().unwrap()[8..], &[0x0b, 54]);
    eprintln!("ble-stage=protocol-reconnected");
    hci.0.borrow_mut().fail = true;
    hci.att(&[0x0a, handle[0], handle[1]]);
    assert!(executor.until(100, || hci.0.borrow().closes == 1));
    assert!(system.radios_quiet());
    assert!((0..system.logs.len())
        .filter_map(|_| system.logs.try_receive().ok())
        .any(|record| matches!(
            record,
            device_runtime::state::LogRecord::Event("ble_runner_failed")
        )));
    eprintln!("ble-stage=protocol-closed");
    let lifetimes = executor.task_lifetimes();
    drop(executor);
    drop(hci);
    drop(system);
    assert!(lifetimes.iter().all(|task| task.strong_count() == 0));
}

#[cfg(feature = "ble")]
#[test]
fn ble_can_restart_after_releasing_its_controller() {
    eprintln!("ble-stage=restart-entered");
    let _clock = clock();
    let system = System::new(Radios {
        wifi: false,
        ble: true,
    });
    let hci = hci::Hci::default();
    let mut executor = Executor::new();
    executor.spawn(device_runtime::ble::ble_loop(
        &system,
        hci::Factory(hci.clone()),
    ));
    system.ble_enabled.signal(true);
    system.battery.signal(73);
    assert!(executor.until(100, || hci.0.borrow().advertising));
    system.ble_enabled.signal(false);
    assert!(executor.until(100, || hci.0.borrow().closes == 1));
    system.ble_enabled.signal(true);
    system.battery.signal(22);
    assert!(executor.until(100, || hci.0.borrow().opens == 2));
    system.ble_enabled.signal(false);
    assert!(executor.until(100, || hci.0.borrow().closes == 2));
    assert!(system.radios_quiet());
    let lifetimes = executor.task_lifetimes();
    drop(executor);
    drop(hci);
    drop(system);
    assert!(lifetimes.iter().all(|task| task.strong_count() == 0));
}

#[cfg(feature = "ble")]
#[test]
fn ble_uses_both_fresh_and_stale_validated_battery_values_without_a_new_measurement() {
    eprintln!("ble-stage=cached-entered");
    let _clock = clock();
    let system = System::new(Radios {
        wifi: false,
        ble: true,
    });
    let hci = hci::Hci::default();
    let mut executor = Executor::new();
    executor.spawn(device_runtime::ble::ble_loop(
        &system,
        hci::Factory(hci.clone()),
    ));
    for battery in [
        Sample::Fresh {
            value: BatteryMillivolts::new(4200).unwrap(),
            measured_at_ms: 0,
        },
        Sample::Stale {
            value: BatteryMillivolts::new(4200).unwrap(),
            measured_at_ms: 0,
        },
    ] {
        eprintln!("ble-stage=cached-starting");
        system.battery.reset();
        let mut app = App::default();
        app.telemetry.battery = battery;
        system.publish(app);
        eprintln!("ble-stage=cached-published");
        system.ble_enabled.signal(true);
        eprintln!("ble-stage=cached-signaled");
        assert!(
            executor.until(100, || hci.0.borrow().advertising),
            "a stored validated battery value needs no new measurement"
        );
        eprintln!("ble-stage=cached-advertising");
        hci.connect();
        hci.0.borrow_mut().acl.clear();
        hci.att(&[0x08, 1, 0, 0xff, 0xff, 0x19, 0x2a]);
        assert!(executor.until(100, || !hci.0.borrow().acl.is_empty()));
        assert_eq!(hci.0.borrow().acl.last().unwrap()[12], 100);
        eprintln!("ble-stage=cached-read");
        let previous = hci.0.borrow().closes;
        system.ble_enabled.signal(false);
        assert!(executor.until(100, || hci.0.borrow().closes == previous + 1));
        eprintln!("ble-stage=cached-closed");
    }
    let lifetimes = executor.task_lifetimes();
    drop(executor);
    drop(hci);
    drop(system);
    assert!(lifetimes.iter().all(|task| task.strong_count() == 0));
}

#[test]
fn console_counts_output_failures_rejects_invalid_progress_and_recovers() {
    let _clock = clock();
    {
        let system = System::new(Radios {
            wifi: false,
            ble: false,
        });
        let board = Board::new();
        board.0.borrow_mut().serial_write = SerialWrite::Pending;
        let mut executor = Executor::new();
        executor.spawn(console_loop(&system, board.clone(), "fixture"));
        executor.advance(500);
        assert_eq!(
            system.lost_logs.load(std::sync::atomic::Ordering::Relaxed),
            0
        );
        assert!(board.output().is_empty());
        board.0.borrow_mut().serial_write = SerialWrite::Partial;
        executor.advance(100);
        assert_eq!(board.output().matches("event=console_ready").count(), 1);
        assert_eq!(
            system.lost_logs.load(std::sync::atomic::Ordering::Relaxed),
            0
        );
        assert!(!board.output().contains("console_read_failed"));
        board.0.borrow_mut().serial_read_failure = true;
        executor.advance(20);
        board.0.borrow_mut().serial_read_failure = false;
        executor.advance(20);
        assert!(board.output().contains("console_read_failed"));
        board.command("unknown-command");
        executor.advance(20);
        assert!(board.output().contains("event=command_rejected"));
    }
    for behavior in [
        SerialWrite::Zero,
        SerialWrite::Oversized,
        SerialWrite::Fault,
        SerialWrite::Pending,
    ] {
        embassy_time::MockDriver::get().reset();
        let system = System::new(Radios {
            wifi: false,
            ble: false,
        });
        let board = Board::new();
        board.0.borrow_mut().serial_write = behavior;
        let mut executor = Executor::new();
        executor.spawn(console_loop(&system, board.clone(), "fixture"));
        assert!(executor.until(1000, || system
            .lost_logs
            .load(std::sync::atomic::Ordering::Relaxed)
            == 1));
        if matches!(
            behavior,
            SerialWrite::Zero | SerialWrite::Oversized | SerialWrite::Fault
        ) {
            assert_eq!(board.0.borrow().serial_writes, 1);
        }
        board.0.borrow_mut().serial_write = SerialWrite::Full;
        system.record(device_runtime::state::LogRecord::Event("healthy_output"));
        executor.advance(20);
        assert!(board.output().contains("healthy_output"));
        assert_eq!(
            system.lost_logs.load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        board.0.borrow_mut().serial_write = SerialWrite::Fault;
        system.record(device_runtime::state::LogRecord::Fault(Fault::Bus));
        executor.advance(20);
        assert_eq!(
            system.lost_logs.load(std::sync::atomic::Ordering::Relaxed),
            2
        );
        board.command("unknown-command");
        executor.advance(20);
        assert_eq!(
            system.lost_logs.load(std::sync::atomic::Ordering::Relaxed),
            3
        );
        board.0.borrow_mut().serial_write = SerialWrite::Full;
        board.command("unknown-command");
        executor.advance(20);
        assert!(board.output().contains("event=command_rejected"));
        assert_eq!(
            system.lost_logs.load(std::sync::atomic::Ordering::Relaxed),
            3
        );
        board.command(&"x".repeat(300));
        executor.advance(200);
        assert!(board.output().contains("event=line_too_long"));
        assert_eq!(
            system.lost_logs.load(std::sync::atomic::Ordering::Relaxed),
            3
        );
        board.0.borrow_mut().serial_write = SerialWrite::Fault;
        board.command(&"x".repeat(300));
        executor.advance(200);
        assert_eq!(
            system.lost_logs.load(std::sync::atomic::Ordering::Relaxed),
            4
        );
    }
}

#[test]
fn diagnostics_reads_extension_status_and_handles_card_removal_and_full_queues() {
    let _clock = clock();
    boot!(board, system, frame, executor, |board: &Board| {
        board.0.borrow_mut().buttons[1] = false;
    });
    executor.advance(300);
    for status in [
        StorageStatus::Ready,
        StorageStatus::Fault,
        StorageStatus::Absent,
    ] {
        board.0.borrow_mut().card = status;
        board.command("diagnose");
        assert!(executor.until(100, || board.0.borrow().led && board.0.borrow().buzzer));
        executor.advance(49);
        assert!(board.0.borrow().led && board.0.borrow().buzzer);
        executor.advance(1);
        assert!(!board.0.borrow().led && !board.0.borrow().buzzer);
        executor.advance(250);
        assert_eq!(system.snapshot().unwrap().diagnostics.storage, status);
        assert!(!system.snapshot().unwrap().storage_busy);
        assert!(!board.0.borrow().led && !board.0.borrow().buzzer);
        assert!(board
            .output()
            .contains("extension_adc millivolts=Fresh { value: 1234"));
    }
    let queued = system.logs.len();
    let lost = system.lost_logs.load(std::sync::atomic::Ordering::Relaxed);
    for _ in 0..100 {
        system.record(device_runtime::state::LogRecord::Event("queue_test"));
    }
    assert_eq!(
        system.lost_logs.load(std::sync::atomic::Ordering::Relaxed),
        lost + 84 + queued as u32
    );
    executor.advance(100);
    assert_eq!(board.output().matches("queue_test").count(), 16 - queued);
    board.0.borrow_mut().buttons[1] = true;
    assert!(executor.until(1000, || board.0.borrow().sleeping.is_some()));
}

#[test]
fn shutdown_flushes_logs_created_by_an_in_flight_radio_operation() {
    let _clock = clock();
    boot!(
        board,
        system,
        frame,
        executor,
        |board: &Board| {
            board.0.borrow_mut().buttons[1] = false;
        },
        Radios {
            wifi: true,
            ble: false
        }
    );
    let wifi = wifi::Factory::default();
    wifi.0.borrow_mut().scan_delay_ms = 1000;
    executor.spawn(device_runtime::wifi::wifi_loop(&system, wifi.clone()));
    executor.advance(300);
    board.command("wifi scan");
    assert!(executor.until(100, || wifi.0.borrow().scans == 1));
    board.0.borrow_mut().buttons[1] = true;
    assert!(executor.until(3000, || board.0.borrow().sleeping.is_some()));
    assert!(
        board.output().contains("event=wifi_scan networks=1"),
        "shutdown discarded the completed scan's log"
    );
    assert!(system.logs.is_empty());
}

#[test]
fn a_console_quiet_acknowledgment_prevents_later_writes_until_a_new_flush_barrier() {
    let _clock = clock();
    let system = System::new(Radios {
        wifi: false,
        ble: false,
    });
    let board = Board::new();
    let mut executor = Executor::new();
    executor.spawn(console_loop(&system, board.clone(), "fixture"));
    system.output_sleep.signal(true);
    executor.advance(20);
    assert!(system.output_quiet.signaled());
    let written = board.0.borrow().serial_writes;
    system.record(device_runtime::state::LogRecord::Event("after_barrier"));
    executor.advance(20);
    assert_eq!(board.0.borrow().serial_writes, written);
    assert_eq!(system.logs.len(), 1);
    system.output_quiet.reset();
    system.output_sleep.signal(true);
    executor.advance(20);
    assert!(system.output_quiet.signaled());
    assert!(system.logs.is_empty());
    assert!(board.output().contains("after_barrier"));
    system.output_quiet.reset();
    board.0.borrow_mut().flush_pending = true;
    system.output_sleep.signal(true);
    executor.advance(500);
    assert!(!system.output_quiet.signaled());
    assert!(system.logs.is_empty());
    board.0.borrow_mut().flush_pending = false;
    executor.advance(20);
    assert!(system.output_quiet.signaled());
    system.output_quiet.reset();
    board.0.borrow_mut().flush_pending = true;
    system.output_sleep.signal(true);
    executor.advance(999);
    assert!(!system.output_quiet.signaled());
    assert!(system.logs.is_empty());
    executor.advance(2);
    assert!(!system.output_quiet.signaled());
    assert_eq!(system.logs.len(), 1);
    board.0.borrow_mut().flush_pending = false;
    system.output_sleep.signal(true);
    executor.advance(20);
    assert!(system.output_quiet.signaled());
    assert!(board.output().contains("event=console_flush_failed"));
}

#[test]
fn an_unavailable_framebuffer_preserves_console_measurement_and_settings_operations() {
    let _clock = clock();
    let mut board = Board::new();
    let (settings, error) = loaded(&mut board);
    let system = System::new(Radios {
        wifi: false,
        ble: false,
    });
    let mut frame = [];
    let mut executor = Executor::new();
    executor.spawn(application_loop(
        &system,
        settings,
        error,
        WakeReason::PowerOn,
        || (1024, 0),
    ));
    executor.spawn(bus_loop(&system, board.clone(), board.clone(), &mut frame));
    executor.spawn(measurements(&system, board.clone(), |_| {}));
    executor.spawn(console_loop(&system, board.clone(), "fixture"));
    executor.advance(100);
    assert_eq!(
        system.snapshot().unwrap().refresh,
        RefreshState::Failed(Fault::Bus)
    );
    assert_eq!(system.presented(), None);
    assert!(matches!(
        system.snapshot().unwrap().telemetry.battery,
        Sample::Fresh { .. }
    ));
    board.command("status");
    board.command("interval 1800");
    executor.advance(100);
    assert!(board.output().contains("event=status"));
    assert_eq!(loaded(&mut board).0.refresh_seconds, 1800);
    assert!(board.0.borrow().sleeping.is_none());
    assert!(board.0.borrow().frames.is_empty());
}
