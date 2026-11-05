use device_preview::*;

#[test]
fn replay_has_identical_state_and_frame_and_rejects_partial_invalid_logs() {
    let log =
        "temperature 2345\nhumidity 5678\nbattery 3900\nnext\nwait\nrefresh\nnext\ncomplete\n";
    let mut first = Preview::default();
    for line in log.lines() {
        first.command(line).unwrap();
    }
    let mut second = Preview::default();
    second.replay(log).unwrap();
    assert_eq!(first.app(), second.app());
    assert_eq!(first.frame(), second.frame());
    let before = *second.app();
    assert!(second.replay("temperature 1800\nnot-an-event").is_err());
    assert_eq!(*second.app(), before);
    for input in [
        "temperature 40000",
        "humidity 10001",
        "battery -1",
        "tick -1",
        "sd maybe",
        "rotation 4",
    ] {
        assert!(second.command(input).is_err(), "{input}");
    }
}

#[test]
fn held_updates_keep_the_presented_frame_until_completion() {
    let mut preview = Preview::default();
    preview.command("temperature 2345").unwrap();
    let original = preview.frame();
    preview.command("wait").unwrap();
    preview.command("next").unwrap();
    assert!(preview.frame() == original);
    preview.command("next").unwrap();
    preview.command("complete").unwrap();
    let first_completion = preview.frame();
    assert!(first_completion != original);
    preview.command("complete").unwrap();
    assert!(preview.frame() != first_completion);
}

#[test]
fn holding_twice_never_publishes_an_unfinished_update() {
    let mut preview = Preview::default();
    let original = preview.frame();
    for command in ["wait", "next", "wait"] {
        preview.command(command).unwrap();
    }
    assert!(
        preview.frame() == original,
        "holding must preserve the presented frame"
    );
}

#[test]
fn a_timed_out_update_cannot_be_published_by_changing_completion_mode() {
    let mut preview = Preview::default();
    let original = preview.frame();
    for command in ["wait", "next", "tick 120000", "immediate"] {
        preview.command(command).unwrap();
    }
    assert!(
        preview.frame() == original,
        "timeout must preserve the presented frame"
    );
    assert!(preview.text().contains("panel timeout"));
    preview.command("refresh").unwrap();
    assert_ne!(preview.frame(), original);
}

#[test]
fn rejected_commands_and_logs_preserve_the_complete_observable_preview() {
    let mut preview = Preview::default();
    for command in ["temperature 2345", "wait", "next"] {
        preview.command(command).unwrap();
    }
    let before = (
        *preview.app(),
        preview.frame(),
        preview.recording(),
        preview.text(),
    );
    for command in [
        "temperature NaN",
        "rotation -1",
        "tick 18446744073709551616",
        "next extra",
        "next\nnext",
    ] {
        assert!(preview.command(command).is_err(), "{command}");
        assert!(
            (
                *preview.app(),
                preview.frame(),
                preview.recording(),
                preview.text()
            ) == before
        );
    }
    for log in ["next\nrotation 4", "temperature 2345\nunknown", "next\0"] {
        assert!(preview.replay(log).is_err());
        assert!(
            (
                *preview.app(),
                preview.frame(),
                preview.recording(),
                preview.text()
            ) == before
        );
    }
}

#[test]
fn invalid_replay_identifies_the_source_line_without_committing_a_prefix() {
    let mut preview = Preview::default();
    preview.command("next").unwrap();
    let before = (*preview.app(), preview.shown(), preview.recording());
    let error = preview
        .replay("\ntemperature 2300\nhumidity 10001")
        .unwrap_err();
    assert_eq!(error.line(), 3);
    assert_eq!(error.to_string(), "Event log line 3: invalid measurement");
    assert_eq!(
        (*preview.app(), preview.shown(), preview.recording()),
        before
    );
}
