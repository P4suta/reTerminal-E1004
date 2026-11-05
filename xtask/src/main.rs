use std::{
    error::Error,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};
use xtask::{monochrome, web_path, FirmwareFeatures};

type TaskResult = Result<(), Box<dyn Error + Send + Sync>>;
const PREVIEW_HOST: &str = "127.0.0.1";
const PREVIEW_SCHEME: &str = "http://";
const PREVIEW_PORT: u16 = 8080;
const NOT_FOUND_STATUS: u16 = 404;
mod dependencies;
mod verification;

#[cfg(feature = "browser")]
mod browser;

fn run(program: &str, args: &[&str]) -> TaskResult {
    println!("phase=command program={program} args={args:?}");
    let status = Command::new(program).args(args).status()?;
    if !status.success() {
        return Err(format!("{program} exited with {status}").into());
    }
    Ok(())
}

fn web_build() -> TaskResult {
    run(
        "cargo",
        &[
            "build",
            "--locked",
            "-p",
            "web-preview",
            "--target",
            "wasm32-unknown-unknown",
            "--release",
        ],
    )?;
    let binary = std::env::var("WASM_BINDGEN").unwrap_or_else(|_| "wasm-bindgen".to_owned());
    run(
        &binary,
        &[
            "--target",
            "web",
            "--out-dir",
            "web/pkg",
            "target/wasm32-unknown-unknown/release/web_preview.wasm",
        ],
    )
}

fn serve(port: u16) -> TaskResult {
    let server = tiny_http::Server::http((PREVIEW_HOST, port))?;
    println!("Preview: {PREVIEW_SCHEME}{PREVIEW_HOST}:{port}");
    for request in server.incoming_requests() {
        let path = web_path(request.url());
        let bytes = path.as_ref().and_then(|path| fs::read(path).ok());
        let response = if let Some(bytes) = bytes {
            let mime = match path
                .and_then(|p| p.extension().map(|s| s.to_string_lossy().into_owned()))
                .as_deref()
            {
                Some("wasm") => "application/wasm",
                Some("js") => "text/javascript",
                Some("css") => "text/css",
                _ => "text/html; charset=utf-8",
            };
            tiny_http::Response::from_data(bytes)
                .with_header(
                    tiny_http::Header::from_bytes("Content-Type", mime)
                        .map_err(|_| "invalid MIME header")?,
                )
                .with_header(
                    tiny_http::Header::from_bytes("Cache-Control", "no-store")
                        .map_err(|_| "invalid cache header")?,
                )
        } else {
            tiny_http::Response::from_data(b"Not found".to_vec()).with_status_code(NOT_FOUND_STATUS)
        };
        request.respond(response)?;
    }
    Ok(())
}

fn docs() -> TaskResult {
    xtask::bindings::validate(&fs::read_to_string("firmware/src/main.rs")?)?;
    monochrome(&fs::read_to_string("web/style.css")?)?;
    for path in [
        "AGENTS.md",
        ".agents/skills/reterminal-e1004/SKILL.md",
        ".agents/skills/reterminal-e1004/references/hardware.md",
        ".agents/skills/reterminal-e1004/references/development.md",
        ".agents/skills/reterminal-e1004/references/verification.md",
        ".agents/skills/reterminal-e1004/references/sources.toml",
        "NOTICE",
    ] {
        if fs::metadata(path)?.len() == 0 {
            return Err(format!("empty development reference: {path}").into());
        }
    }
    let sources: toml::Value = toml::from_str(&fs::read_to_string(
        ".agents/skills/reterminal-e1004/references/sources.toml",
    )?)?;
    let records = sources
        .get("source")
        .and_then(toml::Value::as_array)
        .ok_or("missing source inventory")?;
    if records.is_empty() {
        return Err("empty source inventory".into());
    }
    for record in records {
        for field in [
            "url",
            "retrieved",
            "model",
            "revision",
            "license",
            "unverified",
        ] {
            if record
                .get(field)
                .and_then(toml::Value::as_str)
                .is_none_or(str::is_empty)
            {
                return Err(format!("source is missing {field}").into());
            }
        }
    }
    println!("Development references and monochrome styles passed");
    Ok(())
}

#[derive(Clone, Copy)]
enum FirmwareOperation {
    Build,
    Clippy,
}
fn firmware(features: FirmwareFeatures, operation: FirmwareOperation) -> TaskResult {
    let output = Command::new("rustup")
        .args(["which", "--toolchain", "esp-e1004", "rustc"])
        .output()?;
    if !output.status.success() {
        return Err(
            "install the pinned esp-e1004 toolchain with the development skill instructions".into(),
        );
    }
    let compiler = PathBuf::from(String::from_utf8(output.stdout)?.trim());
    let root = compiler
        .parent()
        .and_then(Path::parent)
        .ok_or("invalid toolchain path")?;
    let gcc = root.join("xtensa-esp-elf/esp-16.1.0_20260609/xtensa-esp-elf/bin");
    let clang = root.join("xtensa-esp32-elf-clang/esp-21.1.3_20260408/esp-clang/lib");
    let mut paths = vec![gcc];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let mut command = Command::new("cargo");
    command
        .args([
            "+esp-e1004",
            match operation {
                FirmwareOperation::Build => "build",
                FirmwareOperation::Clippy => "clippy",
            },
            "--locked",
            "--release",
            "--manifest-path",
            "Cargo.toml",
            "--target",
            "xtensa-esp32s3-none-elf",
            "-Zbuild-std=core,alloc",
            "--no-default-features",
        ])
        .current_dir("firmware")
        .env("PATH", std::env::join_paths(paths)?)
        .env("LIBCLANG_PATH", clang)
        .env("RUSTUP_TOOLCHAIN", "esp-e1004");
    if let Some(features) = features.cargo_features() {
        command.args(["--features", features]);
    }
    if matches!(operation, FirmwareOperation::Clippy) {
        command.args(["--", "-D", "warnings"]);
    }
    println!("phase=firmware features={features:?}");
    let status = command.status()?;
    if !status.success() {
        return Err(format!("firmware build failed: {status}").into());
    }
    if matches!(operation, FirmwareOperation::Build) {
        fs::create_dir_all("target/firmware")?;
        run(
            "espflash",
            &[
                "save-image",
                "--chip",
                "esp32s3",
                "--merge",
                "--flash-size",
                "32mb",
                "--partition-table",
                "firmware/partitions.csv",
                "firmware/target/xtensa-esp32s3-none-elf/release/e1004-firmware",
                &format!("target/firmware/{features:?}.bin"),
                "--skip-update-check",
            ],
        )?;
    }
    Ok(())
}

fn execute() -> TaskResult {
    let mut arguments = std::env::args().skip(1);
    let task = arguments.next().ok_or("expected check, sim, web, web-build, firmware, flash, monitor, docs, proofs, mutations, fuzz, miri, or assurance")?;
    match task.as_str() {
        "ci" => {
            let profile = arguments.next().unwrap_or_else(|| "host".into());
            if !matches!(profile.as_str(), "host" | "device") {
                return Err("ci profile must be host or device".into());
            }
            run("cargo", &["run", "--locked", "-p", "xtask", "--", "check"])?;
            dependencies::check()?;
            run(
                "cargo",
                &[
                    "run",
                    "--locked",
                    "-p",
                    "xtask",
                    "--features",
                    "browser",
                    "--",
                    "browser",
                ],
            )?;
            run(
                "cargo",
                &[
                    "run",
                    "--locked",
                    "-p",
                    "native-preview",
                    "--",
                    "--png",
                    "target/status.png",
                    "--replay",
                    "verification/fixtures/status.events",
                ],
            )?;
            if profile == "device" {
                for features in [
                    FirmwareFeatures::Offline,
                    FirmwareFeatures::Wifi,
                    FirmwareFeatures::Ble,
                    FirmwareFeatures::Both,
                ] {
                    firmware(features, FirmwareOperation::Build)?;
                    firmware(features, FirmwareOperation::Clippy)?;
                }
                verification::check_results()?;
            }
            println!(
                "Delivery profile passed: {profile}; physical device acceptance remains unverified"
            );
            Ok(())
        }
        "check" => {
            let inventory = Command::new("cargo")
                .args([
                    "test",
                    "--locked",
                    "-p",
                    "device-runtime",
                    "--all-features",
                    "--test",
                    "e2e",
                    "--",
                    "--list",
                ])
                .output()?;
            if !inventory.status.success() {
                return Err("firmware task test inventory failed".into());
            }
            xtask::evidence::verify_test_inventory(
                &String::from_utf8(inventory.stdout)?,
                xtask::runtime_test_inventory::CASES,
            )?;
            println!(
                "Runtime E2E inventory: {} required cases",
                xtask::runtime_test_inventory::CASES.len()
            );
            run("cargo", &["fmt", "--all", "--", "--check"])?;
            run(
                "cargo",
                &[
                    "clippy",
                    "--locked",
                    "--workspace",
                    "--all-targets",
                    "--all-features",
                    "--",
                    "-D",
                    "warnings",
                ],
            )?;
            run(
                "cargo",
                &["test", "--locked", "--workspace", "--all-features"],
            )?;
            run(
                "cargo",
                &[
                    "build",
                    "--locked",
                    "-p",
                    "web-preview",
                    "--target",
                    "wasm32-unknown-unknown",
                ],
            )?;
            docs()
        }
        "sim" => {
            let status = Command::new("cargo")
                .args(["run", "--locked", "-p", "native-preview", "--"])
                .args(arguments.map(OsString::from))
                .status()?;
            if !status.success() {
                return Err(format!("simulator exited with {status}").into());
            }
            Ok(())
        }
        "web-build" => web_build(),
        "web" => {
            web_build()?;
            serve(PREVIEW_PORT)
        }
        "serve" => serve(
            arguments
                .next()
                .map(|value| value.parse())
                .transpose()?
                .unwrap_or(PREVIEW_PORT),
        ),
        #[cfg(feature = "browser")]
        "browser" => {
            web_build()?;
            browser::check()
        }
        "docs" => docs(),
        "dependencies" => dependencies::check(),
        "check-assurance" => verification::check_results(),
        "verification-image" => verification::build_image(),
        "firmware" | "firmware-clippy" => {
            let operation = if task == "firmware-clippy" {
                FirmwareOperation::Clippy
            } else {
                FirmwareOperation::Build
            };
            match arguments.next().as_deref().unwrap_or("all") {
                "all" => {
                    for features in [
                        FirmwareFeatures::Offline,
                        FirmwareFeatures::Wifi,
                        FirmwareFeatures::Ble,
                        FirmwareFeatures::Both,
                    ] {
                        firmware(features, operation)?;
                    }
                }
                value => firmware(FirmwareFeatures::parse(value)?, operation)?,
            }
            Ok(())
        }
        "flash" => {
            let port = arguments.next().ok_or("flash requires a serial port")?;
            let features =
                FirmwareFeatures::parse(&arguments.next().unwrap_or_else(|| "offline".into()))?;
            firmware(features, FirmwareOperation::Build)?;
            run(
                "espflash",
                &[
                    "flash",
                    "--port",
                    &port,
                    "--partition-table",
                    "firmware/partitions.csv",
                    "firmware/target/xtensa-esp32s3-none-elf/release/e1004-firmware",
                ],
            )
        }
        "monitor" => run(
            "espflash",
            &[
                "monitor",
                "--port",
                &arguments.next().ok_or("monitor requires a serial port")?,
            ],
        ),
        "verify-container" => {
            verification::snapshot(&arguments.next().ok_or("expected a verification task")?)
        }
        "assurance" => {
            for task in ["proofs-all", "mutations-all", "fuzz", "miri"] {
                verification::bounded(task)?;
            }
            Ok(())
        }
        "mutations" => verification::bounded("mutations-all"),
        "proofs" => verification::bounded("proofs-all"),
        "proofs-core"
        | "proofs-runtime"
        | "runtime-mutation-inventory"
        | "mutations-runtime-shard-0"
        | "mutations-runtime-shard-1"
        | "mutations-runtime-shard-2"
        | "mutations-runtime-shard-3"
        | "miri-runtime"
        | "miri-runtime-boundaries"
        | "miri-runtime-ble"
        | "miri-runtime-ble-restart"
        | "miri-runtime-ble-cached"
        | "proofs-bsp"
        | "proofs-ui"
        | "proofs-preview"
        | "proofs-tools"
        | "mutations-core"
        | "mutations-preview"
        | "mutations-panel"
        | "mutations-core-shard-0"
        | "mutations-core-shard-1"
        | "fuzz"
        | "miri-adapters"
        | "miri-replay-atomicity"
        | "miri-display-hold"
        | "miri-repeat-hold"
        | "miri-timeout-mode"
        | "miri-command-atomicity"
        | "miri-replay-lines"
        | "miri" => {
            let task = if task == "proofs-core" {
                "proofs"
            } else if task == "mutations-core" {
                "mutations"
            } else {
                &task
            };
            verification::bounded(task)
        }
        _ => Err(format!("unknown task: {task}").into()),
    }
}

fn main() -> ExitCode {
    match execute() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
