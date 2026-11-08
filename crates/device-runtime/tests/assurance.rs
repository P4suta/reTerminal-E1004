use device_core::{app::*, telemetry::*};
use serde_json::{json, Value};
use std::{error::Error, fs, path::Path, process::Command};

#[path = "support/fuzz_receipt.rs"]
mod fuzz_receipt;

type CheckResult<T> = Result<T, Box<dyn Error>>;
const EVENT_KINDS: u32 = 9;
const EVENT_DEPTH: u32 = 5;

fn measurement_model() -> u64 {
    let mut transitions = 0;
    for mut word in 0..EVENT_KINDS.pow(EVENT_DEPTH) {
        let mut app = App::default();
        let mut screen = 0;
        let mut measuring = false;
        let mut last_error = None;
        let mut drawing_started = None;
        let mut now_ms = 0u64;
        for _ in 0..EVENT_DEPTH {
            let symbol = word % EVENT_KINDS;
            word /= EVENT_KINDS;
            let event = match symbol {
                0 => {
                    screen = (screen + 1) % 6;
                    AppEvent::Button(Button::Next)
                }
                1 => {
                    screen = (screen + 5) % 6;
                    AppEvent::Button(Button::Previous)
                }
                2 => AppEvent::Button(Button::Refresh),
                3 => {
                    measuring = false;
                    AppEvent::Telemetry(TelemetrySnapshot::default())
                }
                4 => AppEvent::DisplayFinished {
                    id: app.sequence,
                    result: Ok(()),
                },
                5 => AppEvent::RequestSleep,
                6 => {
                    now_ms = REFRESH_INTERVAL_MS;
                    if drawing_started.is_some_and(|started| now_ms - started >= DISPLAY_TIMEOUT_MS)
                    {
                        last_error = Some(Fault::PanelTimeout);
                    }
                    AppEvent::Tick(now_ms)
                }
                7 => {
                    last_error = Some(Fault::SettingsStorage);
                    AppEvent::SettingsFailed
                }
                _ => {
                    last_error = Some(Fault::Storage);
                    AppEvent::StorageChanged(StorageStatus::Fault)
                }
            };
            let prior_error = app.diagnostics.last_error;
            let was_idle = app.refresh == RefreshState::Idle;
            if symbol == 4 {
                drawing_started = None;
            }
            let next = app.transition(event);
            for action in next.actions {
                match action {
                    Action::Measure => {
                        assert!(!measuring, "duplicate measurement");
                        measuring = true;
                    }
                    Action::Display { .. } => {
                        assert!(!measuring, "display preceded measurement");
                        drawing_started = Some(now_ms);
                    }
                    Action::Sleep { .. } => {
                        assert!(!measuring);
                        assert!(!next.state.pending && !next.state.storage_busy);
                        assert_eq!(next.state.refresh, RefreshState::Idle);
                    }
                    Action::StopPanel => drawing_started = None,
                    Action::None | Action::Log(_) => {}
                }
            }
            assert_eq!(next.state.measurement_busy, measuring);
            assert_eq!(next.state.diagnostics.last_error, last_error);
            assert_eq!(
                matches!(next.state.refresh, RefreshState::Updating { .. }),
                drawing_started.is_some()
            );
            if last_error != prior_error
                && next.state.screen == ScreenId::Diagnostics
                && was_idle
                && !measuring
            {
                assert!(
                    next.actions
                        .iter()
                        .any(|action| matches!(action, Action::Display { .. })),
                    "diagnostics must present a changed fault identity"
                );
            }
            assert_eq!(
                next.state.screen,
                [
                    ScreenId::Status,
                    ScreenId::Calendar,
                    ScreenId::Checklist,
                    ScreenId::Notes,
                    ScreenId::Diagnostics,
                    ScreenId::Drawing
                ][screen]
            );
            app = next.state;
            transitions += 1;
        }
    }
    transitions
}

fn run(command: &mut Command) -> CheckResult<String> {
    let output = command.output()?;
    let text = format!(
        "{}\n{}",
        String::from_utf8(output.stdout)?,
        String::from_utf8(output.stderr)?
    );
    eprintln!("{text}");
    if !output.status.success() {
        return Err(format!("verification command failed: {}", output.status).into());
    }
    Ok(text)
}

fn runtime_case(name: &str) -> CheckResult<u64> {
    let text = run(Command::new("cargo").args([
        "test",
        "--locked",
        "-p",
        "device-runtime",
        "--all-features",
        "--test",
        "e2e",
        name,
        "--",
        "--exact",
    ]))?;
    if !text
        .lines()
        .any(|line| line == format!("test {name} ... ok"))
        || !text.contains("1 passed; 0 failed; 0 ignored;")
    {
        return Err(format!("missing completed runtime case: {name}").into());
    }
    Ok(1)
}

fn report(method: &str, bounds: &str, cases: &[(&str, u64)]) -> CheckResult<()> {
    let rustc = run(Command::new("rustc").arg("--version"))?;
    let results = cases.iter().map(|(name, count)| json!({"name":name, "status":"passed", "executed":count, "failed":0, "skipped":0, "inconclusive":0})).collect::<Vec<_>>();
    println!(
        "{}",
        json!({"version":1, "source":std::env::var("DOTFILES_VERIFICATION_SOURCE").unwrap_or_default(), "plan":std::env::var("DOTFILES_VERIFICATION_PLAN").unwrap_or_default(), "method":method, "scope":"affected", "tool_version":rustc.trim(), "target":format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS), "bounds":bounds, "results":results})
    );
    Ok(())
}

fn main() -> CheckResult<()> {
    std::env::set_current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))?;
    match std::env::args().nth(1).as_deref() {
        Some("model") => {
            let steps = measurement_model();
            let restart = runtime_case("screen_policy_survives_real_sleep_and_restart_without_rewriting_unchanged_settings")?;
            let faults = runtime_case(
                "settings_fault_matrix_preserves_sd_status_and_reports_the_actual_operation",
            )?;
            report("model", "9 event kinds, all words of length 5; fresh-task restart and settings-I/O fault matrices", &[("measurement-display-order",steps), ("sleep-restart-matrix",restart), ("storage-fault-matrix",faults)])
        }
        Some("fuzz") => {
            run(Command::new("just").arg("fuzz"))?;
            let receipt: Value =
                serde_json::from_slice(&fs::read("verification/results/bounded-fuzz.json")?)?;
            if !fuzz_receipt::completed(
                receipt["exit"].as_i64(),
                receipt["stdout"].as_str(),
                receipt["stderr"].as_str(),
                10_000,
            ) {
                return Err("incomplete native fuzz campaign".into());
            }
            report(
                "fuzz",
                "cargo-fuzz 0.13.2; 10000 inputs; seed 104; max_len 8192; AddressSanitizer",
                &[("settings-console-corpus", 10000)],
            )
        }
        Some(task) if task.starts_with("mutations-") || task.starts_with("proofs-") => {
            run(Command::new("cargo").args(["run", "--locked", "-p", "xtask", "--", task]))?;
            Ok(())
        }
        None => Ok(()),
        _ => Err("expected model, fuzz, or a declared mutations task".into()),
    }
}
