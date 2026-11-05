#[test]
fn actual_hal_pin_bindings_match_the_bsp_and_reject_wrong_roles() {
    let source = include_str!("../../firmware/src/main.rs");
    assert!(xtask::bindings::validate(source).is_ok());
    for (actual, broken) in [
        ("with_sck(p.GPIO7)", "with_sck(p.GPIO8)"),
        ("with_sck(p.GPIO7)", "with_mosi(p.GPIO7)"),
        ("with_tx(p.GPIO43)", "with_rx(p.GPIO43)"),
        (
            "enable: Output::new(p.GPIO12, Level::High",
            "enable: Output::new(p.GPIO12, Level::Low",
        ),
        (
            "buzzer = Output::new(p.GPIO45, Level::Low",
            "buzzer = Output::new(p.GPIO45, Level::High",
        ),
        ("left: Output::new(p.GPIO10", "left: Output::new(p.GPIO2"),
        ("with_sda(p.GPIO19)", "with_sda(p.GPIO20)"),
        ("Input::new(p.GPIO4", "Input::new(p.GPIO6"),
        ("battery_pin = p.GPIO1", "battery_pin = p.GPIO6"),
        ("with_tx(p.GPIO43)", "with_tx(p.GPIO44)"),
        ("scl: p.GPIO40", "scl: p.GPIO39"),
        ("led = Output::new(p.GPIO48", "led = Output::new(p.GPIO45"),
    ] {
        assert!(source.contains(actual));
        assert!(
            xtask::bindings::validate(&source.replace(actual, broken)).is_err(),
            "accepted {broken}"
        );
    }
    assert!(xtask::bindings::validate("").is_err());
    assert!(xtask::bindings::validate("fn main() {} ").is_err());
}
