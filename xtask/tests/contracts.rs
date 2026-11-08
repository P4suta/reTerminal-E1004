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
    assert_eq!(web_path("/?tools"), Some(PathBuf::from("web/index.html")));
    assert!(web_path("/private.txt?tools").is_none());
    assert_eq!(
        web_path("/pkg/NOTICE.txt"),
        Some(PathBuf::from("web/pkg/NOTICE.txt"))
    );
    assert!(web_path("/pkg/private.txt").is_none());
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
    assert!(monochrome(".field-error:empty, #error:empty { display: none; color: #000; }").is_ok());
    assert!(monochrome("body { color: #000; background: #fff; }").is_ok());
    assert!(monochrome(
        "svg { fill: none; stroke: currentColor; } path { fill: currentColor; stroke: none; }"
    )
    .is_ok());
    assert!(monochrome(".red, #pink { color: black; font-family: red; width: calc(100% - 2px); opacity: 1; box-shadow: none; text-shadow: none; filter: none; backdrop-filter: none; mix-blend-mode: normal; background-blend-mode: normal; }").is_ok());
    assert!(monochrome(
        "@media (min-width: 320px) { #first:hover, #second:focus { border: 2px solid black; } }"
    )
    .is_ok());
    assert!(monochrome(".parent { &:focus, #child { color: white; } }").is_ok());
    for css in [
        "a {color: red;}",
        "a {color: #777;}",
        "a {opacity: .5;}",
        "a {background: linear-gradient(black, white);}",
        "a {box-shadow: 0 0 2px black;}",
        "a {filter: blur(1px);}",
        "a {mix-blend-mode: multiply;}",
        "a {border: 1px solid red;}",
        "a {border: 1px solid #777;}",
        "a {--palette: [#777];}",
        "a {font-family: \"bad\nfont\";}",
        "a {border-left-color: rebeccapurple;}",
        "a {background: RGB(128,128,128);}",
        "a {border: 1px solid oklch(50% 0 0);}",
        "svg {fill: red;}",
        "svg {stroke: #777;}",
        "svg {stroke: rgba(0,0,0,.5);}",
        "body {color: none;}",
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
    assert_eq!(
        FirmwareFeatures::parse("wifi").unwrap().cargo_features(),
        Some("wifi")
    );
    assert_eq!(
        FirmwareFeatures::parse("ble").unwrap().cargo_features(),
        Some("ble")
    );
    assert!(FirmwareFeatures::parse("wfi").is_err());
}
