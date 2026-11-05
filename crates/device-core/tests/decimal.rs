use device_core::telemetry::parse_hundredths;

#[test]
fn decimal_input_is_exact_and_rejects_ambiguous_or_unbounded_numbers() {
    for (text, expected) in [
        ("23.45", 2345),
        ("-0.01", -1),
        ("100", 10000),
        ("-45.0", -4500),
        ("130.00", 13000),
        ("0", 0),
    ] {
        assert_eq!(parse_hundredths(text), Some(expected));
    }
    for text in [
        "",
        "-",
        ".1",
        "1.",
        "1.001",
        "NaN",
        "inf",
        "1e2",
        " 1",
        "1 ",
        "1,2",
        "１２",
        "1.2.3",
        "999999999999999",
    ] {
        assert_eq!(parse_hundredths(text), None, "{text}");
    }
}

#[test]
fn every_supported_sensor_value_round_trips_without_float_rounding() {
    for value in -4500i32..=13000 {
        let text = format!(
            "{}{}.{:02}",
            if value < 0 { "-" } else { "" },
            value.abs() / 100,
            value.abs() % 100
        );
        assert_eq!(parse_hundredths(&text), Some(value));
    }
}

#[test]
fn byte_limits_are_checked_independently_of_numeric_overflow() {
    for (text, expected) in [
        ("00023.45", 2345),
        ("-0023.45", -2345),
        ("00000000", 0),
        ("21474836", 2147483600),
    ] {
        assert_eq!(text.len(), 8);
        assert_eq!(parse_hundredths(text), Some(expected));
    }
    for text in ["000023.45", "-00023.45", "000000001", "21474837"] {
        assert_eq!(parse_hundredths(text), None, "{text}");
    }
}
