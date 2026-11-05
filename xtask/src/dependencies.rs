use std::{error::Error, fs, process::Command};

type Result = std::result::Result<(), Box<dyn Error + Send + Sync>>;
const ADVISORIES: &str = "https://github.com/RustSec/advisory-db";
const DATABASE_DIRECTORY: &str = "advisory-db-3157b0e258782691";
const GIT_SHA1_HEX_BYTES: usize = 40;

pub fn check() -> Result {
    let reference = Command::new("git")
        .args(["ls-remote", ADVISORIES, "HEAD"])
        .output()?;
    if !reference.status.success() {
        return Err("cannot identify the RustSec snapshot".into());
    }
    let output = String::from_utf8(reference.stdout)?;
    let commit = output
        .split_whitespace()
        .next()
        .ok_or("empty advisory reference")?;
    if commit.len() != GIT_SHA1_HEX_BYTES || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("invalid advisory reference".into());
    }
    let root = std::env::current_dir()?
        .join("target")
        .join(format!("advisories-{commit}"));
    let database = root.join(DATABASE_DIRECTORY);
    fs::create_dir_all(&root)?;
    if !database.exists() {
        crate::run(
            "git",
            &[
                "clone",
                "--depth",
                "1",
                ADVISORIES,
                database.to_str().ok_or("non-UTF-8 advisory path")?,
            ],
        )?;
    }
    let head = Command::new("git")
        .arg("-C")
        .arg(&database)
        .args(["rev-parse", "HEAD"])
        .output()?;
    if !head.status.success() || String::from_utf8(head.stdout)?.trim() != commit {
        return Err("advisory snapshot changed while cloning; rerun the check".into());
    }
    println!("RustSec advisory snapshot: {commit}");
    for manifest in ["Cargo.toml", "firmware/Cargo.toml", "fuzz/Cargo.toml"] {
        crate::run("cargo", &["fetch", "--locked", "--manifest-path", manifest])?;
        let status = Command::new("cargo")
            .args([
                "deny",
                "--locked",
                "--offline",
                "--config",
                "deny.toml",
                "--manifest-path",
                manifest,
                "--all-features",
                "check",
                "--warn",
                "unmaintained",
                "advisories",
                "licenses",
                "sources",
            ])
            .env("E1004_ADVISORY_DB", &root)
            .status()?;
        if !status.success() {
            return Err(format!("dependency check failed for {manifest}").into());
        }
    }
    crate::run(
        "gitleaks",
        &[
            "dir",
            "--redact",
            "--no-banner",
            "--report-format",
            "json",
            "--report-path",
            "target/secrets.json",
            ".",
        ],
    )?;
    println!("Dependency vulnerability, license, source, and secret checks passed; maintenance advisories remain visible warnings");
    Ok(())
}
