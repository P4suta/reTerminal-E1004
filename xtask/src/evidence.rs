use serde::Deserialize;
use std::collections::BTreeSet;
const VERIFICATION_TARGET_DIRECTORY: &str = "/tmp/e1004-checks";
pub struct MutationShard {
    pub task: &'static str,
    pub receipt: &'static str,
    pub selector: &'static str,
}
pub const CORE_MUTATION_SHARDS: &[MutationShard] = &[
    MutationShard {
        task: "mutations-core-shard-0",
        receipt: "core-mutations-0",
        selector: "0/2",
    },
    MutationShard {
        task: "mutations-core-shard-1",
        receipt: "core-mutations-1",
        selector: "1/2",
    },
];
pub const RUNTIME_MUTATION_SHARDS: &[MutationShard] = &[
    MutationShard {
        task: "mutations-runtime-shard-0",
        receipt: "runtime-mutations-0",
        selector: "0/4",
    },
    MutationShard {
        task: "mutations-runtime-shard-1",
        receipt: "runtime-mutations-1",
        selector: "1/4",
    },
    MutationShard {
        task: "mutations-runtime-shard-2",
        receipt: "runtime-mutations-2",
        selector: "2/4",
    },
    MutationShard {
        task: "mutations-runtime-shard-3",
        receipt: "runtime-mutations-3",
        selector: "3/4",
    },
];
pub struct InterpreterCase {
    pub task: &'static str,
    pub filter: &'static str,
    pub artifacts: &'static [&'static str],
}
pub const MIRI_BLE_CASES: &[InterpreterCase] = &[
    InterpreterCase {
        task: "miri-runtime-ble",
        filter:
            "real_ble_host_advertises_serves_att_battery_reads_notifies_and_releases_the_controller",
        artifacts: &["miri-runtime-ble.json"],
    },
    InterpreterCase {
        task: "miri-runtime-ble-restart",
        filter: "ble_can_restart_after_releasing_its_controller",
        artifacts: &["miri-runtime-ble-restart.json"],
    },
    InterpreterCase {
        task: "miri-runtime-ble-cached",
        filter: "ble_uses_both_fresh_and_stale_validated_battery_values_without_a_new_measurement",
        artifacts: &["miri-runtime-ble-cached.json"],
    },
];
pub const MIRI_PREVIEW_CASES: &[InterpreterCase] = &[
    InterpreterCase {
        task: "miri-replay-atomicity",
        filter: "replay_has_identical_state_and_frame_and_rejects_partial_invalid_logs",
        artifacts: &["miri-replay-atomicity.json"],
    },
    InterpreterCase {
        task: "miri-display-hold",
        filter: "held_updates_keep_the_presented_frame_until_completion",
        artifacts: &["miri-display-hold.json"],
    },
    InterpreterCase {
        task: "miri-repeat-hold",
        filter: "holding_twice_never_publishes_an_unfinished_update",
        artifacts: &["miri-repeat-hold.json"],
    },
    InterpreterCase {
        task: "miri-timeout-mode",
        filter: "a_timed_out_update_cannot_be_published_by_changing_completion_mode",
        artifacts: &["miri-timeout-mode.json"],
    },
    InterpreterCase {
        task: "miri-command-atomicity",
        filter: "rejected_commands_and_logs_preserve_the_complete_observable_preview",
        artifacts: &["miri-command-atomicity.json"],
    },
    InterpreterCase {
        task: "miri-replay-lines",
        filter: "invalid_replay_identifies_the_source_line_without_committing_a_prefix",
        artifacts: &["miri-replay-lines.json"],
    },
];

pub fn verify_test_inventory(report: &str, expected: &[&str]) -> Result<(), &'static str> {
    let names = report
        .lines()
        .filter_map(|line| line.strip_suffix(": test"))
        .collect::<Vec<_>>();
    let unique = names.iter().copied().collect::<BTreeSet<_>>();
    let required = expected.iter().copied().collect::<BTreeSet<_>>();
    if names.is_empty()
        || names.len() != unique.len()
        || expected.len() != required.len()
        || unique != required
    {
        return Err("interpreter selection does not cover the complete test inventory");
    }
    Ok(())
}
const VERIFICATION_PRODUCERS: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "verification/Dockerfile",
    "verification/Host.Dockerfile",
    "vendor/trouble-host",
    "verification/inventory.toml",
    "xtask/src/evidence.rs",
    "xtask/src/verification.rs",
];

pub fn verification_inputs<'a>(scope: &[&'a str]) -> Vec<&'a str> {
    let mut inputs = scope.to_vec();
    inputs.extend(VERIFICATION_PRODUCERS);
    inputs.sort_unstable();
    inputs.dedup();
    inputs
}

pub fn receipt_scope(name: &str) -> Result<&'static [&'static str], &'static str> {
    if name == "miri-inventory" || MIRI_PREVIEW_CASES.iter().any(|case| case.task == name) {
        return Ok(&[
            "crates/preview",
            "crates/device-core/src",
            "crates/device-ui/src",
        ]);
    }
    match name {
        "runtime-proofs"
        | "runtime-mutation-inventory"
        | "runtime-mutations-0"
        | "runtime-mutations-1"
        | "runtime-mutations-2"
        | "runtime-mutations-3"
        | "miri-runtime-measurements"
        | "miri-runtime-ble"
        | "miri-runtime-ble-restart"
        | "miri-runtime-ble-cached"
        | "miri-runtime-packets"
        | "miri-runtime-policy" => Ok(&[
            "crates/device-runtime",
            "crates/device-core",
            "crates/device-ui",
            "crates/e1004-bsp",
            "crates/t133a01",
            "firmware/src",
        ]),
        "kani-version" | "mutants-version" => Ok(&[]),
        "core-proofs" => Ok(&["crates/device-core/src", "crates/device-core/Cargo.toml"]),
        "bsp-proofs" => Ok(&[
            "crates/e1004-bsp/src",
            "crates/device-core/src",
            "firmware/src/effects.rs",
        ]),
        "ui-proofs" => Ok(&["crates/device-ui", "crates/device-core/src"]),
        "negative-control" => Ok(&[
            "verification/negative/src",
            "verification/negative/Cargo.toml",
            "verification/negative/Cargo.lock",
        ]),
        "preview-proofs" => Ok(&[
            "crates/preview/src",
            "crates/preview/Cargo.toml",
            "crates/device-core/src",
            "crates/device-ui/src",
        ]),
        "tool-proofs" => Ok(&["xtask/src/lib.rs", "xtask/Cargo.toml"]),
        "core-mutations-0" | "core-mutations-1" => Ok(&["crates/device-core"]),
        "preview-mutations" => Ok(&[
            "crates/preview",
            "crates/device-core/src",
            "crates/device-ui/src",
        ]),
        "panel-mutations" | "miri-panel" => Ok(&["crates/t133a01", "crates/device-core/src"]),
        "bsp-mutations" | "miri-bsp" => Ok(&[
            "crates/device-core/src",
            "crates/e1004-bsp",
            "firmware/src/effects.rs",
        ]),
        "miri-ui" => Ok(&["crates/device-ui", "crates/device-core/src"]),
        "bounded-fuzz" => Ok(&[
            "crates/device-runtime",
            "fuzz/fuzz_targets",
            "fuzz/corpus",
            "fuzz/Cargo.toml",
            "fuzz/Cargo.lock",
            "crates/device-ui/src",
            "crates/preview/src",
            "crates/device-core/src",
            "crates/e1004-bsp/src",
            "xtask/src/lib.rs",
            "xtask/src/bindings.rs",
        ]),
        _ => Err("unknown verification receipt scope"),
    }
}

pub fn verification_command(arguments: &[&str]) -> std::process::Command {
    let mut command = std::process::Command::new("cargo");
    command.args(arguments).env("CARGO_INCREMENTAL", "0");
    if arguments.first() == Some(&"mutants") {
        for variable in [
            "CARGO_TARGET_DIR",
            "CARGO_BUILD_TARGET_DIR",
            "CARGO_BUILD_BUILD_DIR",
        ] {
            command.env_remove(variable);
        }
    } else {
        command.env("CARGO_TARGET_DIR", VERIFICATION_TARGET_DIRECTORY);
    }
    command
}

#[derive(Clone, Copy, Deserialize)]
pub struct MutationCounts {
    pub total_mutants: u64,
    pub caught: u64,
    pub unviable: u64,
    pub missed: u64,
    pub timeout: u64,
    pub success: u64,
}
impl MutationCounts {
    pub fn complete(self) -> bool {
        self.caught > 0
            && self.missed == 0
            && self.timeout == 0
            && self.success == 0
            && self.caught.checked_add(self.unviable) == Some(self.total_mutants)
    }
}

pub fn validate_mutations(text: &str, version: &str, files: &[&str]) -> Result<u64, &'static str> {
    let report: serde_json::Value =
        serde_json::from_str(text).map_err(|_| "invalid mutation JSON")?;
    let counts: MutationCounts =
        serde_json::from_value(report.clone()).map_err(|_| "missing mutation totals")?;
    if files.is_empty()
        || !counts.complete()
        || report["cargo_mutants_version"].as_str() != Some(version)
    {
        return Err("mutation totals or version are incomplete");
    }
    let outcomes = report["outcomes"]
        .as_array()
        .ok_or("missing mutation outcomes")?;
    let mut baseline = 0;
    let mut caught = 0;
    let mut unviable = 0;
    let mut names = BTreeSet::new();
    let mut covered = BTreeSet::new();
    for outcome in outcomes {
        if outcome["scenario"].as_str() == Some("Baseline") {
            if outcome["summary"].as_str() != Some("Success") {
                return Err("mutation baseline did not pass");
            }
            baseline += 1;
        } else {
            let mutant = &outcome["scenario"]["Mutant"];
            let file = mutant["file"].as_str().ok_or("missing mutant file")?;
            let name = mutant["name"].as_str().ok_or("missing mutant identity")?;
            if !files.contains(&file) || !names.insert(name) {
                return Err("unexpected or duplicate mutant");
            }
            covered.insert(file);
            match outcome["summary"].as_str() {
                Some("CaughtMutant") => caught += 1,
                Some("Unviable") => unviable += 1,
                _ => return Err("unresolved mutation outcome"),
            }
        }
    }
    if baseline != 1
        || caught != counts.caught
        || unviable != counts.unviable
        || !files.iter().all(|file| covered.contains(file))
    {
        return Err("mutation outcomes do not cover the declared work");
    }
    Ok(counts.total_mutants)
}

pub fn result_artifacts(task: &str) -> Result<&'static [&'static str], &'static str> {
    if let Some(case) = MIRI_PREVIEW_CASES
        .iter()
        .chain(MIRI_BLE_CASES)
        .find(|case| case.task == task)
    {
        return Ok(case.artifacts);
    }
    match task {
        "proofs-runtime" => Ok(&["runtime-proofs.json"]),
        "runtime-mutation-inventory" => Ok(&["runtime-mutation-inventory.json"]),
        "mutations-runtime-shard-0" => Ok(&["runtime-mutations-0.json", "runtime-mutations-0"]),
        "mutations-runtime-shard-1" => Ok(&["runtime-mutations-1.json", "runtime-mutations-1"]),
        "mutations-runtime-shard-2" => Ok(&["runtime-mutations-2.json", "runtime-mutations-2"]),
        "mutations-runtime-shard-3" => Ok(&["runtime-mutations-3.json", "runtime-mutations-3"]),
        "miri-runtime-boundaries" => Ok(&[
            "miri-runtime-measurements.json",
            "miri-runtime-policy.json",
            "miri-runtime-packets.json",
        ]),
        "proofs" => Ok(&[
            "kani-version.json",
            "core-proofs.json",
            "bsp-proofs.json",
            "ui-proofs.json",
            "negative-control.json",
        ]),
        "proofs-bsp" => Ok(&["bsp-proofs.json"]),
        "proofs-ui" => Ok(&["ui-proofs.json"]),
        "proofs-preview" => Ok(&["preview-proofs.json"]),
        "proofs-tools" => Ok(&["tool-proofs.json"]),
        "mutations" => Ok(&[
            "core-mutations-0.json",
            "core-mutations-0",
            "core-mutations-1.json",
            "core-mutations-1",
        ]),
        "mutations-core-shard-0" => Ok(&["core-mutations-0.json", "core-mutations-0"]),
        "mutations-core-shard-1" => Ok(&["core-mutations-1.json", "core-mutations-1"]),
        "mutations-preview" => Ok(&["preview-mutations.json", "preview-mutations"]),
        "mutations-panel" => Ok(&["panel-mutations.json", "panel-mutations"]),
        "mutations-bsp" => Ok(&["bsp-mutations.json", "bsp-mutations"]),
        "fuzz" => Ok(&["bounded-fuzz.json"]),
        "miri" => Ok(&[
            "miri-inventory.json",
            "miri-bsp.json",
            "miri-panel.json",
            "miri-ui.json",
            "miri-replay-atomicity.json",
            "miri-display-hold.json",
            "miri-repeat-hold.json",
            "miri-timeout-mode.json",
            "miri-command-atomicity.json",
            "miri-replay-lines.json",
        ]),
        "miri-adapters" => Ok(&[
            "miri-inventory.json",
            "miri-bsp.json",
            "miri-panel.json",
            "miri-ui.json",
        ]),
        _ => Err("unknown verification artifact owner"),
    }
}

#[cfg(kani)]
mod proofs {
    use super::*;
    #[kani::proof]
    #[kani::unwind(2)]
    fn mutation_verdict_requires_nonempty_complete_counts_without_overflow() {
        let counts = MutationCounts {
            total_mutants: kani::any(),
            caught: kani::any(),
            unviable: kani::any(),
            missed: kani::any(),
            timeout: kani::any(),
            success: kani::any(),
        };
        let wide_total = u128::from(counts.caught) + u128::from(counts.unviable);
        assert_eq!(
            counts.complete(),
            counts.caught > 0
                && counts.missed == 0
                && counts.timeout == 0
                && counts.success == 0
                && wide_total == u128::from(counts.total_mutants)
        );
        kani::cover!(counts.complete() && counts.unviable > 0);
        kani::cover!(!counts.complete() && counts.caught == u64::MAX);
    }
}
