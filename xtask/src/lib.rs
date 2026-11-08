use std::path::PathBuf;
pub mod evidence;
pub mod preview_server;

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
    let url = url.split_once('?').map_or(url, |(path, _)| path);
    if url == "/" || url == "/index.html" {
        return Some("web/index.html".into());
    }
    if url == "/style.css" {
        return Some("web/style.css".into());
    }
    if let Some(file) = url.strip_prefix("/pkg/") {
        if matches!(
            file,
            "NOTICE.txt" | "LICENSE-MIT.txt" | "LICENSE-APACHE.txt"
        ) {
            return Some(PathBuf::from("web/pkg").join(file));
        }
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

mod style;
pub use style::monochrome;
pub mod bindings;
pub mod runtime_test_inventory;
