use device_core::settings::ResumePolicy;
use e1004_bsp::console::{parse, Command, LineInput, LineResult, ParseError};

#[test]
fn console_and_line_input() {
    for (line, expected) in [
        ("status", Command::Status),
        ("diagnose", Command::Diagnose),
        ("refresh", Command::Refresh),
        ("sleep", Command::Sleep),
        ("wifi scan", Command::WifiScan),
        ("wifi disconnect", Command::WifiDisconnect),
        ("ble on", Command::Ble(true)),
        ("ble off", Command::Ble(false)),
    ] {
        assert_eq!(parse(line), Ok(expected));
    }
    let boundary = format!("{}status", " ".repeat(250));
    assert_eq!(boundary.len(), 256);
    assert_eq!(parse(&boundary), Ok(Command::Status));
    assert_eq!(parse(&format!(" {boundary}")), Err(ParseError::TooLong));
    assert_eq!(
        parse("wifi connect network\tsecret  \r\n"),
        Ok(Command::WifiConnect {
            ssid: "network",
            password: "secret  "
        })
    );
    for ssid_length in [0, 1, 31, 32, 33] {
        for password_length in [0, 1, 62, 63, 64] {
            let ssid = "s".repeat(ssid_length);
            let password = "p".repeat(password_length);
            let line = format!("wifi connect {ssid}\t{password}");
            assert_eq!(
                parse(&line),
                if ssid_length == 0 || ssid_length > 32 || password_length > 63 {
                    Err(ParseError::Invalid)
                } else {
                    Ok(Command::WifiConnect {
                        ssid: &ssid,
                        password: &password,
                    })
                }
            );
        }
    }
    assert_eq!(parse("resume unknown"), Err(ParseError::Invalid));
    for (line, expected) in [
        ("resume last", ResumePolicy::Last),
        ("resume status", ResumePolicy::Status),
    ] {
        assert_eq!(parse(line), Ok(Command::Resume(expected)));
        let mut input = LineInput::default();
        for byte in line.bytes() {
            assert_eq!(input.push(byte), LineResult::Pending);
        }
        assert_eq!(input.push(b'\n'), LineResult::Complete);
        assert_eq!(parse(input.line().unwrap()), Ok(Command::Resume(expected)));
        for suffix in 0u8..=127 {
            let text = format!("{line}{}", char::from(suffix));
            assert_eq!(
                parse(&text),
                if char::from(suffix).is_whitespace() {
                    Ok(Command::Resume(expected))
                } else {
                    Err(ParseError::Invalid)
                }
            );
        }
    }
    for line in [
        "resume",
        "resume ",
        "resume last extra",
        "resume Status",
        "resume unknown",
    ] {
        assert!(parse(line).is_err());
    }
}
