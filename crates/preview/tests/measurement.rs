use device_core::{app::*, telemetry::*};
use device_preview::Preview;

#[test]
fn refresh_finishes_simulated_measurement_before_starting_a_display() {
    let mut preview = Preview::default();
    preview.command("temperature 2100").unwrap();
    preview.command("wait").unwrap();
    preview.command("refresh").unwrap();
    assert!(!preview.app().measurement_busy);
    assert!(matches!(
        preview.app().refresh,
        RefreshState::Updating { .. }
    ));
    assert_eq!(
        preview.app().telemetry.temperature,
        Sample::Fresh {
            value: Temperature::new(2100).unwrap(),
            measured_at_ms: 0
        }
    );
    preview.command("complete").unwrap();
    assert_eq!(preview.shown().0.telemetry, preview.app().telemetry);
    assert_eq!(preview.app().refresh, RefreshState::Idle);
}
