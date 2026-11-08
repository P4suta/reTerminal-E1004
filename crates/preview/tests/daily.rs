use device_preview::Preview;

#[test]
fn daily_commands_are_usable_recordable_and_atomic_on_invalid_input() {
    let mut preview = Preview::default();
    for command in [
        "clock 2026-11-08T09:00:00",
        "screen calendar",
        "hold next",
        "hold previous",
        "task set 1 Buy groceries",
        "task set 6 Water plants",
        "task done 1",
        "note set Remember the library books.",
        "screen checklist",
    ] {
        assert!(preview.command(command).is_ok(), "{command}");
    }
    assert!(preview.text().contains("Buy groceries"));
    assert!(preview.text().contains("Remember the library books."));
    let log = preview.recording();
    let frame = preview.frame();
    let mut replay = Preview::default();
    replay.replay(&log).unwrap();
    assert_eq!(replay.app(), preview.app());
    assert_eq!(replay.frame(), frame);
    for command in [
        "task set 0 invalid",
        "task set 7 invalid",
        "note set 日本語",
        "screen unknown",
    ] {
        assert!(preview.command(command).is_err(), "{command}");
        assert_eq!(preview.recording(), log);
        assert_eq!(preview.frame(), frame);
    }
}

#[test]
fn queued_daily_edits_do_not_replace_a_frame_until_its_update_completes() {
    let mut preview = Preview::default();
    preview.command("task set 1 Buy groceries").unwrap();
    preview.command("screen checklist").unwrap();
    let before = preview.frame();
    preview.command("wait").unwrap();
    preview.command("task done 1").unwrap();
    assert_eq!(preview.frame(), before);
    preview.command("complete").unwrap();
    assert_ne!(preview.frame(), before);
    let completed = preview.frame();
    preview.command("note set Pack lunch.").unwrap();
    preview.command("screen notes").unwrap();
    preview.command("timeout").unwrap();
    assert_eq!(preview.frame(), completed);
}
