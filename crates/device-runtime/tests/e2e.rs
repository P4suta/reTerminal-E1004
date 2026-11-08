mod support;
#[test]
fn automatic_gallery_restarts_advance_without_rewriting_flash_preferences() {
    use device_core::{
        media::*,
        settings::{crc32, Settings},
    };
    let _clock = clock();
    let mut board = Board::new();
    let settings = Settings {
        screen: ScreenId::Gallery,
        gallery: GallerySettings {
            slideshow: SlideInterval::new(60),
            ..Default::default()
        },
        ..Default::default()
    };
    e1004_bsp::storage::save(&mut board, settings).unwrap();
    let mut storage = vec![255; 0x900000];
    for (slot, number, color) in [(0usize, 1, 0x22), (1, 3, 0x55)] {
        let pixels = vec![color; FRAME_BYTES];
        let record = ImageRecord::image(
            ImageId::new(number).unwrap(),
            slot as u64,
            device_core::geometry::Rotation::Portrait,
            crc32(&pixels),
        );
        let offset = slot * 0x100000;
        storage[offset..offset + HEADER_BYTES].copy_from_slice(&record.encode());
        storage[offset + HEADER_BYTES..offset + HEADER_BYTES + FRAME_BYTES]
            .copy_from_slice(&pixels);
    }
    board.0.borrow_mut().media = Some(storage);
    let writes = board
        .0
        .borrow()
        .trace
        .iter()
        .filter(|line| line.starts_with("flash "))
        .count();
    for cycle in 0..4 {
        embassy_time::MockDriver::get().reset();
        board.0.borrow_mut().sleeping = None;
        let (settings, error) = loaded(&mut board);
        let system = System::new(Radios {
            wifi: false,
            ble: false,
        });
        let mut frame = vec![PACKED_WHITE; FRAME_BYTES];
        let mut image = vec![PACKED_WHITE; FRAME_BYTES];
        let mut executor = Executor::new();
        executor.spawn(application_loop(
            &system,
            settings,
            error,
            WakeReason::Timer,
            || (12345, 654321),
        ));
        executor.spawn(bus_loop_with_media(
            &system,
            board.clone(),
            board.clone(),
            &mut frame,
            &mut image,
        ));
        executor.spawn(measurements(&system, board.clone(), |_| {}));
        executor.spawn(buttons_loop(&system, board.clone()));
        executor.spawn(console_loop(&system, board.clone(), "fixture"));
        executor.spawn(ticks(&system));
        assert!(executor.until(3000, || board.0.borrow().sleeping == Some(60)));
        assert_eq!(
            system
                .presented()
                .unwrap()
                .gallery
                .settings()
                .selected
                .number(),
            if cycle % 2 == 0 { 3 } else { 1 }
        );
        assert_eq!(system.snapshot().unwrap().settings(), settings);
        assert_eq!(
            board
                .0
                .borrow()
                .trace
                .iter()
                .filter(|line| line.starts_with("flash "))
                .count(),
            writes
        );
    }
}
#[test]
fn a_late_image_storage_reply_cannot_acknowledge_a_new_command() {
    let _clock = clock();
    let system = System::new(Radios {
        wifi: false,
        ble: false,
    });
    let board = Board::new();
    let result = core::cell::Cell::new(None);
    let mut io = board.clone();
    let mut executor = Executor::new();
    executor.spawn(async {
        result.set(Some(
            device_runtime::console::command(&system, &mut io, "control 5 image abort 7").await,
        ));
        core::future::pending::<()>().await;
    });
    executor.advance(0);
    let device_runtime::state::BusRequest::Media(request) = system.bus.try_receive().unwrap()
    else {
        panic!("missing media request")
    };
    system
        .media_reply
        .signal(device_runtime::state::MediaReply {
            id: request.id + 1,
            result: Ok(0),
        });
    executor.advance(0);
    assert_eq!(result.get(), None);
    system
        .media_reply
        .signal(device_runtime::state::MediaReply {
            id: request.id,
            result: Err(e1004_bsp::media::MediaError::Missing),
        });
    assert!(executor.until(10, || result.get().is_some()));
    assert_eq!(result.get(), Some(false));
    assert!(board.output().ends_with("reply 5 error\r\n"));
}
#[test]
fn an_unreadable_gallery_index_does_not_replace_a_retained_panel_image() {
    let _clock = clock();
    let mut board = Board::new();
    {
        let mut device = board.0.borrow_mut();
        device.buttons[1] = false;
        device.media_fault = true;
        device.slots[0] = device_core::settings::encode(
            device_core::settings::Settings {
                screen: ScreenId::Gallery,
                ..Default::default()
            },
            0,
        );
    }
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
    executor.spawn(ticks(&system));
    assert!(executor.until(3000, || system
        .snapshot()
        .is_some_and(|app| app.refresh == RefreshState::Failed(Fault::Storage))));
    assert!(board.0.borrow().frames.is_empty());
    assert!(system.presented().is_none());
    board.0.borrow_mut().media_fault = false;
    system
        .events
        .try_send(AppEvent::Hold(Button::Refresh))
        .unwrap();
    assert!(executor.until(3000, || system
        .presented()
        .is_some_and(|app| app.screen == ScreenId::Gallery)));
    assert_eq!(board.0.borrow().frames.len(), 1);
}
use device_core::{
    app::*,
    geometry::{FRAME_BYTES, PACKED_WHITE},
    telemetry::*,
};
use device_runtime::{
    buttons::buttons_loop, console::console_loop, policy::Radios, state::System, tasks::*,
};
use support::*;

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

#[test]
fn browser_control_commits_color_images_and_recovers_interrupted_transfers_through_production_tasks(
) {
    use device_core::{
        media::{GalleryEvent, ImageId, SlideInterval},
        settings::crc32,
    };
    let _clock = clock();
    let mut board = Board::new();
    board.0.borrow_mut().buttons[1] = false;
    let (settings, error) = loaded(&mut board);
    let system = System::new(Radios {
        wifi: false,
        ble: false,
    });
    let mut frame = vec![PACKED_WHITE; FRAME_BYTES];
    let mut image = vec![PACKED_WHITE; FRAME_BYTES];
    let mut executor = Executor::new();
    executor.spawn(application_loop(
        &system,
        settings,
        error,
        WakeReason::PowerOn,
        || (12345, 654321),
    ));
    executor.spawn(bus_loop_with_media(
        &system,
        board.clone(),
        board.clone(),
        &mut frame,
        &mut image,
    ));
    executor.spawn(measurements(&system, board.clone(), |_| {}));
    executor.spawn(buttons_loop(&system, board.clone()));
    executor.spawn(console_loop(&system, board.clone(), "fixture"));
    executor.spawn(ticks(&system));
    executor.advance(1000);
    let mut sequence = 0u32;
    let mut command = |executor: &mut Executor<'_>, body: &str, ok: bool| {
        sequence += 1;
        board.0.borrow_mut().uart_tx.clear();
        board.command(&format!("control {sequence} {body}"));
        let reply = format!("reply {sequence} {}\r\n", if ok { "ok" } else { "error" });
        assert!(
            executor.until(1000, || board
                .0
                .borrow()
                .uart_tx
                .ends_with(reply.as_bytes())),
            "{body}: {}",
            board.output()
        );
    };
    command(&mut executor, "awake 600", true);
    command(&mut executor, "help", true);
    assert!(board.output().contains("image commit ID"));
    let mut pixels = vec![0x33; FRAME_BYTES];
    command(
        &mut executor,
        &format!("image begin 10 3 0 {}", crc32(&pixels)),
        true,
    );
    for (i, bytes) in pixels.chunks(64).enumerate() {
        use core::fmt::Write;
        let mut line = format!("image data 10 {} ", i * 64);
        for byte in bytes {
            write!(&mut line, "{byte:02x}").unwrap();
        }
        command(&mut executor, &line, true);
        assert!(system.snapshot().unwrap().gallery.busy());
        assert_eq!(board.0.borrow().sleeping, None);
    }
    command(&mut executor, "image commit 11", false);
    command(&mut executor, "image commit 10", true);
    assert!(
        executor.until(2000, || system
            .presented()
            .is_some_and(|app| app.screen == ScreenId::Gallery
                && app.gallery.settings().selected.number() == 3))
    );
    assert!(board
        .0
        .borrow()
        .frames
        .last()
        .unwrap()
        .iter()
        .all(|v| *v == 0x33));
    let catalog = system.snapshot().unwrap().gallery.catalog();
    command(&mut executor, "image begin 20 3 0 0", true);
    command(&mut executor, "image data 20 0 00000000", true);
    command(&mut executor, "image abort 21", false);
    command(&mut executor, "image abort 20", true);
    assert_eq!(system.snapshot().unwrap().gallery.catalog(), catalog);
    command(&mut executor, "image begin 30 3 0 0", true);
    executor.advance(61000);
    assert!(!system.snapshot().unwrap().gallery.busy());
    assert_eq!(system.snapshot().unwrap().gallery.catalog(), catalog);
    assert_eq!(board.0.borrow().sleeping, None);
    command(&mut executor, "image import 3", false);
    assert_eq!(system.snapshot().unwrap().gallery.catalog(), catalog);
    pixels.fill(0x55);
    let mut file = device_core::media::ImageRecord::image(
        ImageId::new(3).unwrap(),
        0,
        device_core::geometry::Rotation::Portrait,
        crc32(&pixels),
    )
    .encode()
    .to_vec();
    file.extend_from_slice(&pixels);
    board.0.borrow_mut().sd_image = Some(file.clone());
    command(&mut executor, "image import 3", true);
    let expected_checksum = crc32(&pixels);
    assert!(
        executor.until(2000, || system.presented().is_some_and(|app| app
            .gallery
            .view()
            .image
            .is_some_and(
                |(_, descriptor)| descriptor.checksum == expected_checksum
            )))
    );
    let catalog = system.snapshot().unwrap().gallery.catalog();
    file[40] ^= 1;
    board.0.borrow_mut().sd_image = Some(file);
    command(&mut executor, "image import 3", false);
    assert_eq!(system.snapshot().unwrap().gallery.catalog(), catalog);
    system
        .events
        .try_send(AppEvent::Gallery(GalleryEvent::Slideshow(
            SlideInterval::new(60),
        )))
        .unwrap();
    command(&mut executor, "status", true);
    assert!(board.output().contains("event=image number=3"));
    assert!(board
        .output()
        .contains("event=shown screen=Gallery image=3"));
    command(&mut executor, "awake 0", true);
    board.0.borrow_mut().buttons[1] = true;
    assert!(executor.until(4000, || board.0.borrow().sleeping == Some(60)));
    drop(executor);
    let (settings, error) = loaded(&mut board);
    assert_eq!(error, None);
    assert_eq!(settings.gallery.selected.number(), 3);
    assert_eq!(settings.gallery.slideshow, SlideInterval::new(60));
    let store = e1004_bsp::media::MediaStore::open(&mut board).unwrap();
    assert_eq!(store.catalog(), catalog);
    store
        .load(&mut board, ImageId::new(3).unwrap(), &mut image)
        .unwrap();
    assert_eq!(image, pixels);
}

#[test]
fn daily_content_selection_and_calendar_survive_console_editing_sleep_and_restart() {
    use device_core::settings::Settings;
    let _clock = clock();
    for screen in [ScreenId::Calendar, ScreenId::Checklist, ScreenId::Notes] {
        embassy_time::MockDriver::get().reset();
        boot!(board, system, frame, executor, |board: &Board| {
            board.0.borrow_mut().buttons[1] = false;
        });
        executor.advance(1000);
        for command in [
            "interval 600",
            "rotation 3",
            "task set 1 Buy groceries",
            "task set 6 Water plants",
            "task done 1",
            "note set Return the library books.",
        ] {
            board.command(command);
        }
        board.command(match screen {
            ScreenId::Calendar => "screen calendar",
            ScreenId::Checklist => "screen checklist",
            _ => "screen notes",
        });
        assert!(executor
            .until(3000, || system.snapshot().is_some_and(|app| app.screen
                == screen
                && app.daily.note().as_str() == "Return the library books.")));
        if screen == ScreenId::Calendar {
            board.command("hold next");
        }
        if screen == ScreenId::Checklist {
            for _ in 0..5 {
                board.command("hold next");
            }
        }
        executor.advance(1500);
        assert!(executor.until(3000, || system
            .snapshot()
            .is_some_and(|app| app.refresh == RefreshState::Idle)));
        let expected = system.snapshot().unwrap().settings();
        assert_eq!(expected.refresh_seconds, 600);
        assert_eq!(expected.rotation, 3);
        assert_eq!(expected.daily.counts(), (1, 2));
        if screen == ScreenId::Calendar {
            assert!(expected.calendar_month.is_some());
        }
        if screen == ScreenId::Checklist {
            assert_eq!(expected.selected_task.number(), 6);
        }
        board.command("status");
        executor.advance(200);
        assert!(board.output().contains("Return the library books."));
        board.0.borrow_mut().buttons[1] = true;
        assert!(executor.until(3000, || board.0.borrow().sleeping.is_some()));
        drop(executor);
        let (settings, error) = loaded(&mut board);
        assert_eq!(error, None);
        assert_eq!(settings, expected);
        assert_ne!(settings, Settings::default());
        board.0.borrow_mut().sleeping = None;
        let reboot = System::new(device_runtime::policy::Radios {
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
        assert!(executor.until(3000, || reboot
            .presented()
            .is_some_and(|app| app.screen == screen && app.settings() == expected)));
    }
}

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
        assert!(system.events.is_empty());
        executor.advance(200);
        assert!(system.events.is_empty());
        board.0.borrow_mut().buttons[index] = true;
        executor.advance(80);
        assert_eq!(system.events.try_receive(), Ok(AppEvent::Button(button)));
        board.0.borrow_mut().buttons[index] = false;
        executor.advance(60);
        assert!(system.events.is_empty());
        executor.advance(820);
        assert_eq!(system.events.try_receive(), Ok(AppEvent::Hold(button)));
        executor.advance(1000);
        assert!(system.events.is_empty());
        board.0.borrow_mut().buttons[index] = true;
        executor.advance(80);
    }
}

#[test]
fn sleep_arming_waits_for_each_button_release_to_finish_gesture_classification() {
    let _clock = clock();
    for (index, button) in [Button::Previous, Button::Refresh, Button::Next]
        .into_iter()
        .enumerate()
    {
        for held in [false, true] {
            embassy_time::MockDriver::get().reset();
            let system = System::new(Radios {
                wifi: false,
                ble: false,
            });
            let board = Board::new();
            let mut executor = Executor::new();
            executor.spawn(buttons_loop(&system, board.clone()));
            board.0.borrow_mut().buttons[index] = false;
            executor.advance(60);
            if held {
                executor.advance(820);
                assert_eq!(system.events.try_receive(), Ok(AppEvent::Hold(button)));
            }
            board.0.borrow_mut().buttons[index] = true;
            system.button_sleep.signal(true);
            executor.advance(20);
            assert_eq!(system.button_armed.try_take(), Some(false));
            assert_eq!(board.0.borrow().armed, [false; 3]);
            executor.advance(40);
            if held {
                assert!(system.events.is_empty());
            } else {
                assert_eq!(system.events.try_receive(), Ok(AppEvent::Button(button)));
            }
            system.button_sleep.signal(true);
            executor.advance(20);
            assert_eq!(system.button_armed.try_take(), Some(true));
            assert_eq!(board.0.borrow().armed, [true; 3]);
            assert!(system.events.is_empty());
        }
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

#[test]
fn switching_resume_policy_saves_the_active_screen_without_an_extra_sleep_write() {
    use device_core::settings::{ResumePolicy, Settings};
    let _clock = clock();
    for (initial, resume, command) in [
        (ResumePolicy::Status, ResumePolicy::Last, "resume last"),
        (ResumePolicy::Last, ResumePolicy::Status, "resume status"),
    ] {
        for screen in ScreenId::ALL {
            embassy_time::MockDriver::get().reset();
            boot!(board, system, frame, executor, |board: &Board| {
                let mut device = board.0.borrow_mut();
                device.buttons[1] = false;
                device.slots[0] = device_core::settings::encode(
                    Settings {
                        refresh_seconds: 1800,
                        rotation: 3,
                        resume: initial,
                        screen: ScreenId::Status,
                        ..Settings::default()
                    },
                    7,
                );
            });
            executor.advance(1000);
            system
                .events
                .try_send(AppEvent::SelectScreen(screen))
                .unwrap();
            assert!(executor.until(3000, || system
                .presented()
                .is_some_and(|app| app.screen == screen)
                && system
                    .snapshot()
                    .is_some_and(|app| app.refresh == RefreshState::Idle)));
            board.command(command);
            assert!(executor.until(1000, || system
                .snapshot()
                .is_some_and(|app| app.persisted_settings.resume == resume)));
            let (saved, error) = loaded(&mut board);
            assert_eq!(error, None);
            assert_eq!(saved.resume, resume);
            assert_eq!(
                saved.screen,
                if resume == ResumePolicy::Last {
                    screen
                } else {
                    ScreenId::Status
                }
            );
            assert_eq!(saved.refresh_seconds, 1800);
            assert_eq!(saved.rotation, 3);
            let writes = board
                .0
                .borrow()
                .trace
                .iter()
                .filter(|line| line.starts_with("flash "))
                .count();
            board.0.borrow_mut().buttons[1] = true;
            assert!(executor.until(3000, || board.0.borrow().sleeping.is_some()));
            assert_eq!(
                board
                    .0
                    .borrow()
                    .trace
                    .iter()
                    .filter(|line| line.starts_with("flash "))
                    .count(),
                writes
            );
        }
    }
}

#[test]
fn boot_waits_for_measurements_and_draws_once() {
    let _clock = clock();
    for failed_sensor in [false, true] {
        embassy_time::MockDriver::get().reset();
        boot!(board, system, frame, executor, |board: &Board| {
            board.0.borrow_mut().sensor_failure =
                failed_sensor.then_some(device_runtime::measurements::IoFailure::Absent);
        });
        assert!(executor.until(5000, || board.0.borrow().sleeping.is_some()));
        let device = board.0.borrow();
        assert_eq!(
            device.frames.len(),
            1,
            "boot must wait for the completed measurement"
        );
        let shown = system.presented().unwrap();
        assert_eq!(
            matches!(shown.telemetry.temperature, Sample::Missing),
            failed_sensor
        );
        assert!(!matches!(shown.telemetry.temperature, Sample::Unavailable));
        assert!(!shown.measurement_busy);
    }
}

#[test]
fn settings_fault_matrix_preserves_sd_status_and_reports_the_actual_operation() {
    let _clock = clock();
    let mut baseline = Board::new();
    e1004_bsp::storage::save(&mut baseline, Default::default()).unwrap();
    let save_operations = baseline.0.borrow().storage_calls;
    assert!(save_operations > 0);
    let mut injected = 0;
    for card in [
        StorageStatus::Absent,
        StorageStatus::Ready,
        StorageStatus::Fault,
    ] {
        for operation in 1..=save_operations {
            embassy_time::MockDriver::get().reset();
            boot!(board, system, frame, executor, |board: &Board| {
                let mut device = board.0.borrow_mut();
                device.buttons[1] = false;
                device.card = card;
                device.slots[0] = device_core::settings::encode(Default::default(), 0);
            });
            executor.advance(1000);
            board.command("diagnose");
            executor.advance(500);
            assert_eq!(system.snapshot().unwrap().diagnostics.storage, card);
            {
                let mut device = board.0.borrow_mut();
                device.storage_calls = 0;
                device.storage_fault_at = Some(operation);
            }
            board.command("interval 1800");
            assert!(
                executor.until(2000, || board.0.borrow().storage_fault_hits == 1
                    && system.events.is_empty()
                    && system.snapshot().is_some_and(|app| !app.storage_busy))
            );
            executor.advance(100);
            let app = system.snapshot().unwrap();
            assert_eq!(
                app.diagnostics.storage, card,
                "settings failure must preserve the SD result: operation={operation}"
            );
            assert_eq!(
                format!("{:?}", app.diagnostics.last_error),
                "Some(SettingsStorage)"
            );
            assert_eq!(app.refresh_seconds, 900);
            assert!(board.output().contains("fault=SettingsStorage"));
            board.0.borrow_mut().storage_fault_at = None;
            board.command("interval 1800");
            assert!(executor.until(2000, || system
                .snapshot()
                .is_some_and(|app| app.refresh_seconds == 1800)));
            assert_eq!(board.0.borrow().storage_fault_hits, 1);
            assert_eq!(system.snapshot().unwrap().diagnostics.storage, card);
            injected += board.0.borrow().storage_fault_hits;
        }
    }
    assert_eq!(injected, save_operations * 3);
}

#[test]
fn screen_policy_survives_real_sleep_and_restart_without_rewriting_unchanged_settings() {
    let _clock = clock();
    for policy in ["last", "status"] {
        for screen in ScreenId::ALL {
            for wake in [WakeReason::Timer, WakeReason::Button, WakeReason::PowerOn] {
                embassy_time::MockDriver::get().reset();
                boot!(board, system, frame, executor, |board: &Board| {
                    board.0.borrow_mut().buttons[1] = false;
                });
                executor.advance(1000);
                board.command(&format!("resume {policy}"));
                executor.advance(500);
                system
                    .events
                    .try_send(AppEvent::SelectScreen(screen))
                    .unwrap();
                assert!(executor.until(3000, || system
                    .presented()
                    .is_some_and(|app| app.screen == screen)
                    && system
                        .snapshot()
                        .is_some_and(|app| app.refresh == RefreshState::Idle)));
                board.0.borrow_mut().buttons[1] = true;
                assert!(executor.until(3000, || board.0.borrow().sleeping.is_some()));
                drop(executor);
                let (settings, error) = loaded(&mut board);
                assert_eq!(error, None);
                let writes = board
                    .0
                    .borrow()
                    .trace
                    .iter()
                    .filter(|line| line.starts_with("flash "))
                    .count();
                board.0.borrow_mut().sleeping = None;
                let reboot = System::new(Radios {
                    wifi: false,
                    ble: false,
                });
                let mut executor = Executor::new();
                executor.spawn(application_loop(&reboot, settings, error, wake, || {
                    (12345, 654321)
                }));
                executor.spawn(bus_loop(&reboot, board.clone(), board.clone(), &mut frame));
                executor.spawn(measurements(&reboot, board.clone(), |_| {}));
                executor.spawn(buttons_loop(&reboot, board.clone()));
                executor.spawn(console_loop(&reboot, board.clone(), "fixture"));
                executor.spawn(ticks(&reboot));
                assert!(executor.until(5000, || board.0.borrow().sleeping.is_some()));
                let expected = if policy == "last" {
                    screen
                } else {
                    ScreenId::Status
                };
                assert_eq!(
                    reboot.presented().unwrap().screen,
                    expected,
                    "policy={policy}, wake={wake:?}"
                );
                assert_eq!(
                    board
                        .0
                        .borrow()
                        .trace
                        .iter()
                        .filter(|line| line.starts_with("flash "))
                        .count(),
                    writes,
                    "unchanged wake cycles must not wear flash"
                );
            }
        }
    }
}

#[test]
fn rapid_measurement_and_navigation_requests_coalesce_before_sleep() {
    let _clock = clock();
    boot!(board, system, frame, executor, |board: &Board| {
        board.0.borrow_mut().buttons[1] = false;
    });
    executor.advance(1000);
    assert!(executor.until(3000, || system
        .snapshot()
        .is_some_and(|app| app.refresh == RefreshState::Idle)));
    let before = board.0.borrow().frames.len();
    system
        .events
        .try_send(AppEvent::Button(Button::Refresh))
        .unwrap();
    executor.advance(1);
    for index in 0..1000 {
        system
            .events
            .try_send(AppEvent::Button(if index % 2 == 0 {
                Button::Refresh
            } else {
                Button::Next
            }))
            .unwrap();
        executor.advance(0);
    }
    board.0.borrow_mut().buttons[1] = true;
    assert!(executor.until(5000, || board.0.borrow().sleeping.is_some()));
    assert_eq!(
        board.0.borrow().frames.len() - before,
        1,
        "requests during measurement must share one sample and final display"
    );
    assert_eq!(system.presented().unwrap().screen, ScreenId::Notes);
    assert!(!system.snapshot().unwrap().measurement_busy);
}

#[cfg(feature = "ble")]
#[test]
fn every_radio_combination_rotation_and_screen_survives_boot_input_sleep_and_wake() {
    let _clock = clock();
    for mode in 0..4 {
        for rotation in 0..4 {
            for screen in 0..ScreenId::ALL.len() {
                embassy_time::MockDriver::get().reset();
                let capabilities = Radios {
                    wifi: mode & 1 != 0,
                    ble: mode & 2 != 0,
                };
                let wake = [WakeReason::PowerOn, WakeReason::Timer, WakeReason::Button][screen % 3];
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
                                ..Default::default()
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
                let (settings, error) = loaded(&mut board);
                assert_eq!(error, None);
                assert_eq!(settings.rotation, rotation);
                let selected = ScreenId::ALL[screen];
                let writes = board
                    .0
                    .borrow()
                    .trace
                    .iter()
                    .filter(|line| line.starts_with("flash "))
                    .count();
                board.0.borrow_mut().sleeping = None;
                let reboot = System::new(capabilities);
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
                if capabilities.wifi {
                    executor.spawn(device_runtime::wifi::wifi_loop(&reboot, wifi.clone()));
                }
                if capabilities.ble {
                    executor.spawn(device_runtime::ble::ble_loop(
                        &reboot,
                        hci::Factory(hci.clone()),
                    ));
                }
                assert!(executor.until(5000, || board.0.borrow().sleeping.is_some()));
                let restored = reboot.presented().unwrap();
                assert_eq!(restored.screen, selected);
                assert_eq!(restored.rotation as u8, rotation);
                assert_eq!(restored.refresh_seconds, interval);
                assert_eq!(restored.diagnostics.wake, WakeReason::Timer);
                assert_eq!(
                    board
                        .0
                        .borrow()
                        .trace
                        .iter()
                        .filter(|line| line.starts_with("flash "))
                        .count(),
                    writes
                );
                assert!(reboot.radios_quiet());
            }
        }
    }
}

#[test]
fn failed_screen_retention_is_reported_once_and_preserves_a_committed_screen() {
    let _clock = clock();
    let mut baseline = Board::new();
    e1004_bsp::storage::save(&mut baseline, Default::default()).unwrap();
    let operations = baseline.0.borrow().storage_calls;
    for operation in 1..=operations {
        embassy_time::MockDriver::get().reset();
        boot!(board, system, frame, executor, |board: &Board| {
            let mut device = board.0.borrow_mut();
            device.buttons[1] = false;
            device.slots[0] = device_core::settings::encode(Default::default(), 0);
            device.card = StorageStatus::Ready;
        });
        executor.advance(1000);
        board.command("diagnose");
        executor.advance(500);
        system
            .events
            .try_send(AppEvent::SelectScreen(ScreenId::Drawing))
            .unwrap();
        assert!(executor.until(3000, || system
            .presented()
            .is_some_and(|app| app.screen == ScreenId::Drawing)
            && system
                .snapshot()
                .is_some_and(|app| app.refresh == RefreshState::Idle)));
        {
            let mut device = board.0.borrow_mut();
            device.storage_calls = 0;
            device.storage_fault_at = Some(operation);
            device.buttons[1] = true;
        }
        assert!(executor.until(5000, || board.0.borrow().sleeping.is_some()));
        assert_eq!(board.0.borrow().storage_fault_hits, 1);
        assert_eq!(board.0.borrow().storage_calls, operation);
        let app = system.snapshot().unwrap();
        assert_eq!(app.diagnostics.storage, StorageStatus::Ready);
        assert_eq!(app.diagnostics.last_error, Some(Fault::SettingsStorage));
        assert!(board.output().contains("fault=SettingsStorage"));
        drop(executor);
        let (settings, error) = loaded(&mut board);
        assert_eq!(error, None);
        assert!(matches!(
            settings.screen,
            ScreenId::Status | ScreenId::Drawing
        ));
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
            == RadioStatus::Connected
            && board.output().contains("wifi_scan networks=1")
            && board.output().contains("signal_dbm=-42"))
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
    assert_eq!(system.snapshot().unwrap().screen, ScreenId::Calendar);
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
    for (busy, cancel, transport, valid_frame, expected) in [
        (true, false, false, true, Fault::PanelTimeout),
        (true, true, false, true, Fault::PanelTimeout),
        (false, false, true, true, Fault::Bus),
        (false, false, false, false, Fault::Bus),
    ] {
        embassy_time::MockDriver::get().reset();
        let system = System::new(Radios {
            wifi: false,
            ble: false,
        });
        let board = Board::new();
        board.0.borrow_mut().busy_forever = busy;
        board.0.borrow_mut().spi_failure = transport;
        let frame = vec![PACKED_WHITE; if valid_frame { FRAME_BYTES } else { 0 }];
        let mut hardware = board.clone();
        let mut panel = t133a01::Panel::default();
        let result = core::cell::Cell::new(None);
        let mut executor = Executor::new();
        executor.spawn(async {
            result.set(Some(
                panel_update(&system, &mut panel, &mut hardware, &frame).await,
            ));
            core::future::pending::<()>().await;
        });
        executor.advance(1);
        if cancel {
            system.stop_panel.signal(());
        }
        assert!(executor.until(DISPLAY_TIMEOUT_MS + 1, || result.get().is_some()));
        assert_eq!(result.get(), Some(Err(expected)));
        assert!(!board.0.borrow().panel_on);
        assert!(board.0.borrow().selected.is_none());
    }
    embassy_time::MockDriver::get().reset();
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
        assert!(board.output().contains("Enter help"));
        assert_eq!(board.output().matches("event=command_rejected").count(), 1);
        board.command("help");
        executor.advance(20);
        assert!(board
            .output()
            .contains("interval SECONDS (60 to 86400 seconds)"));
        board.0.borrow_mut().uart_rx.extend([u8::MAX, b'\n']);
        executor.advance(20);
        assert!(board.output().contains("kind=InvalidText"));
        board.command(&"x".repeat(257));
        board.command("help");
        executor.advance(200);
        assert!(board.output().contains("event=line_too_long message="));
        assert_eq!(board.output().matches("event=help").count(), 2);
        assert!(system.events.is_empty() && system.bus.is_empty());
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
    for behavior in [SerialWrite::Full, SerialWrite::Fault] {
        embassy_time::MockDriver::get().reset();
        let system = System::new(Radios {
            wifi: false,
            ble: false,
        });
        let board = Board::new();
        board.0.borrow_mut().serial_write = SerialWrite::Full;
        let mut executor = Executor::new();
        executor.spawn(console_loop(&system, board.clone(), "fixture"));
        executor.advance(20);
        while system
            .bus
            .try_send(device_runtime::state::BusRequest::Diagnose)
            .is_ok()
        {}
        board.0.borrow_mut().serial_write = behavior;
        for (input, response) in [
            (b"unknown-command\n".to_vec(), "event=command_rejected"),
            (vec![u8::MAX, b'\n'], "kind=InvalidText"),
            (
                format!("{}\n", "x".repeat(300)).into_bytes(),
                "event=line_too_long",
            ),
            (b"interval 60\n".to_vec(), "Command not completed"),
        ] {
            let before_lost = system.lost_logs.load(std::sync::atomic::Ordering::Relaxed);
            let before_output = board.output().len();
            board.0.borrow_mut().uart_rx.extend(input);
            executor.advance(200);
            assert_eq!(
                system.lost_logs.load(std::sync::atomic::Ordering::Relaxed),
                before_lost + u32::from(matches!(behavior, SerialWrite::Fault))
            );
            let output = board.output();
            if matches!(behavior, SerialWrite::Full) {
                assert_eq!(output[before_output..].matches(response).count(), 1);
            } else {
                assert_eq!(output.len(), before_output);
            }
        }
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
