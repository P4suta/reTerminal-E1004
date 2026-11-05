use std::path::PathBuf;
use xtask::*;

#[test]
fn status_fixture_matches_the_reviewed_framebuffer_digest() {
    use sha2::{Digest, Sha256};
    let fixture = include_str!("../../verification/fixtures/status.events");
    let expected = include_str!("../../verification/fixtures/status.sha256").trim();
    let mut preview = device_preview::Preview::default();
    preview.replay(fixture).unwrap();
    assert_eq!(format!("{:x}", Sha256::digest(preview.frame())), expected);
}

#[test]
fn static_server_exposes_only_preview_assets() {
    assert_eq!(web_path("/"), Some(PathBuf::from("web/index.html")));
    assert_eq!(
        web_path("/pkg/web_preview_bg.wasm"),
        Some(PathBuf::from("web/pkg/web_preview_bg.wasm"))
    );
    for url in [
        "/../Cargo.toml",
        "/%2e%2e/Cargo.toml",
        "/pkg/../../firmware/Cargo.toml",
        "/.git/config",
        "/Cargo.toml",
        "/pkg/a%00.js",
    ] {
        assert!(web_path(url).is_none(), "{url}");
    }
}

#[test]
fn monochrome_design_gate_rejects_prohibited_styles() {
    assert!(monochrome("body { color: #000; background: #fff; }").is_ok());
    for css in [
        "a {color: red;}",
        "a {color: #777;}",
        "a {opacity: .5;}",
        "a {background: linear-gradient(black, white);}",
        "a {box-shadow: 0 0 2px black;}",
        "a {filter: blur(1px);}",
        "a {mix-blend-mode: multiply;}",
    ] {
        assert!(monochrome(css).is_err(), "{css}");
    }
}

#[test]
fn firmware_feature_matrix_has_no_silent_fallback() {
    assert_eq!(
        FirmwareFeatures::parse("offline").unwrap().cargo_features(),
        None
    );
    assert_eq!(
        FirmwareFeatures::parse("wifi,ble")
            .unwrap()
            .cargo_features(),
        Some("wifi,ble,coex")
    );
    assert!(FirmwareFeatures::parse("wfi").is_err());
}
