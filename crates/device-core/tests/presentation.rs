use device_core::{presentation::Presentation, telemetry::Fault};

#[test]
fn only_a_matching_success_can_replace_the_presented_snapshot() {
    let mut display = Presentation::new(10);
    display.start(7, 20);
    display.finish(6, Ok(()));
    assert_eq!(display.shown(), 10);
    assert_eq!(display.active_id(), Some(7));
    display.finish(7, Err(Fault::PanelTimeout));
    assert_eq!(display.shown(), 10);
    assert_eq!(display.active_id(), None);
    display.finish(7, Ok(()));
    assert_eq!(display.shown(), 10);
    display.start(8, 30);
    display.finish(8, Ok(()));
    assert_eq!(display.shown(), 30);
    display.start(9, 40);
    display.cancel();
    display.finish(9, Ok(()));
    assert_eq!(display.shown(), 30);
}
