use e1004_bsp::input::{Gesture, GestureEvent, HOLD_MS};

#[test]
fn taps_wait_for_debounced_release_and_holds_never_emit_a_tap_or_repeat() {
    for hold in [false, true] {
        let mut gesture = Gesture::default();
        assert!(gesture.is_idle());
        for time in [0, 20, 40] {
            assert_eq!(gesture.update(true, time), None);
        }
        let until = if hold { 40 + HOLD_MS } else { 780 };
        assert!(!gesture.is_idle());
        for time in (60..until).step_by(20) {
            assert_eq!(gesture.update(true, time), None);
        }
        let release = if hold {
            assert_eq!(gesture.update(true, 40 + HOLD_MS), Some(GestureEvent::Hold));
            assert_eq!(gesture.update(true, 4000), None);
            5000
        } else {
            780
        };
        assert_eq!(gesture.update(false, release), None);
        assert_eq!(gesture.update(false, release + 20), None);
        assert!(!gesture.is_idle());
        assert_eq!(
            gesture.update(false, release + 40),
            if hold { None } else { Some(GestureEvent::Tap) }
        );
        assert_eq!(gesture.update(false, release + 60), None);
        assert!(gesture.is_idle());
        for time in [6000, 6020, 6040] {
            assert_eq!(gesture.update(true, time), None);
        }
        for time in [6060, 6080] {
            assert_eq!(gesture.update(false, time), None);
        }
        assert_eq!(gesture.update(false, 6100), Some(GestureEvent::Tap));
    }
}
