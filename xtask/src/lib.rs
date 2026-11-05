use std::path::PathBuf;
pub mod evidence;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FirmwareFeatures {
    Offline,
    Wifi,
    Ble,
    Both,
}

impl FirmwareFeatures {
    pub fn parse(value: &str) -> Result<Self, &'static str> {
        match value {
            "offline" => Ok(Self::Offline),
            "wifi" => Ok(Self::Wifi),
            "ble" => Ok(Self::Ble),
            "wifi,ble" => Ok(Self::Both),
            _ => Err("expected offline, wifi, ble, or wifi,ble"),
        }
    }
    pub fn cargo_features(self) -> Option<&'static str> {
        match self {
            Self::Offline => None,
            Self::Wifi => Some("wifi"),
            Self::Ble => Some("ble"),
            Self::Both => Some("wifi,ble,coex"),
        }
    }
}

pub fn web_path(url: &str) -> Option<PathBuf> {
    if url == "/" || url == "/index.html" {
        return Some("web/index.html".into());
    }
    if url == "/style.css" {
        return Some("web/style.css".into());
    }
    if let Some(file) = url.strip_prefix("/pkg/") {
        if !file.is_empty()
            && file
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'-')
            && !file.contains("..")
            && [".js", ".wasm", ".ts"]
                .iter()
                .any(|suffix| file.ends_with(suffix))
        {
            return Some(PathBuf::from("web/pkg").join(file));
        }
    }
    None
}

pub fn monochrome(css: &str) -> Result<(), &'static str> {
    let mut input = cssparser::ParserInput::new(css);
    scan_css(&mut cssparser::Parser::new(&mut input))
        .map_err(|_| "style exceeds the black-and-white design contract")
}

fn scan_css<'i>(
    parser: &mut cssparser::Parser<'i, '_>,
) -> Result<(), cssparser::ParseError<'i, ()>> {
    use cssparser::Token;
    let mut in_value = false;
    while let Ok(token) = parser.next_including_whitespace_and_comments().cloned() {
        match token {
            Token::Colon => in_value = true,
            Token::Semicolon => in_value = false,
            Token::Hash(value) | Token::IDHash(value) if in_value => {
                if !["000", "000000", "fff", "ffffff"]
                    .contains(&value.to_ascii_lowercase().as_str())
                {
                    return Err(parser.new_custom_error(()));
                }
            }
            Token::Ident(value) => {
                let name = value.to_ascii_lowercase();
                if [
                    "color",
                    "background",
                    "background-color",
                    "border-color",
                    "outline-color",
                ]
                .contains(&name.as_str())
                {
                    let state = parser.state();
                    if parser.expect_colon().is_ok() {
                        match parser.next()?.clone() {
                            Token::Hash(v) | Token::IDHash(v)
                                if ["000", "000000", "fff", "ffffff"]
                                    .contains(&v.to_ascii_lowercase().as_str()) => {}
                            Token::Ident(v) if v == "black" || v == "white" => {}
                            _ => return Err(parser.new_custom_error(())),
                        }
                    } else {
                        parser.reset(&state);
                    }
                }
                if [
                    "opacity",
                    "box-shadow",
                    "text-shadow",
                    "filter",
                    "backdrop-filter",
                    "mix-blend-mode",
                    "background-blend-mode",
                ]
                .contains(&name.as_str())
                {
                    parser.expect_colon()?;
                    match parser.next()?.clone() {
                        Token::Number { value: 1.0, .. } if name == "opacity" => {}
                        Token::Ident(v) if v == "none" || v == "normal" => {}
                        _ => return Err(parser.new_custom_error(())),
                    }
                }
            }
            Token::Function(name) => {
                if name.contains("gradient")
                    || ["rgb", "rgba", "hsl", "hsla", "color", "color-mix"].contains(&name.as_ref())
                {
                    return Err(parser.new_custom_error(()));
                }
                parser.parse_nested_block(scan_css)?;
            }
            Token::CurlyBracketBlock | Token::ParenthesisBlock | Token::SquareBracketBlock => {
                parser.parse_nested_block(scan_css)?;
                in_value = false;
            }
            Token::BadString(_)
            | Token::BadUrl(_)
            | Token::CloseParenthesis
            | Token::CloseSquareBracket
            | Token::CloseCurlyBracket => return Err(parser.new_custom_error(())),
            _ => {}
        }
    }
    Ok(())
}
pub mod bindings;
pub mod runtime_test_inventory;
