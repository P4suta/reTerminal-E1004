use serde_json::{json, Value};
use xtask::evidence::verification_command;
use xtask::evidence::verification_inputs;

#[test]
fn receipts_cover_the_verification_producer_and_locked_toolchain_as_well_as_the_product() {
    let inputs = verification_inputs(&["crates/preview/src", "Cargo.lock"]);
    for required in [
        "crates/preview/src",
        "Cargo.lock",
        "Cargo.toml",
        "rust-toolchain.toml",
        "verification/Dockerfile",
        "verification/Host.Dockerfile",
        "vendor/trouble-host",
        "verification/inventory.toml",
        "xtask/src/evidence.rs",
        "xtask/src/verification.rs",
    ] {
        assert!(inputs.contains(&required), "missing {required}");
    }
    assert_eq!(
        inputs.iter().filter(|path| **path == "Cargo.lock").count(),
        1
    );
}

#[test]
fn mutation_workers_cannot_inherit_shared_cargo_output_directories() {
    let command = verification_command(&["mutants", "--jobs", "2"]);
    let environment = command
        .get_envs()
        .collect::<std::collections::BTreeMap<_, _>>();
    for variable in [
        "CARGO_TARGET_DIR",
        "CARGO_BUILD_TARGET_DIR",
        "CARGO_BUILD_BUILD_DIR",
    ] {
        assert_eq!(
            environment.get(std::ffi::OsStr::new(variable)),
            Some(&None),
            "{variable}"
        );
    }
    let normal = verification_command(&["test"]);
    assert!(normal
        .get_envs()
        .any(|(key, value)| key == "CARGO_TARGET_DIR" && value.is_some()));
}

#[test]
fn every_produced_receipt_has_one_authoritative_scope() {
    for task in [
        "proofs",
        "proofs-preview",
        "proofs-tools",
        "proofs-runtime",
        "miri-runtime-boundaries",
        "miri-runtime-ble",
        "miri-runtime-ble-restart",
        "miri-runtime-ble-cached",
        "mutations-runtime-shard-0",
        "mutations-runtime-shard-1",
        "mutations-runtime-shard-2",
        "mutations-runtime-shard-3",
        "mutations",
        "mutations-preview",
        "mutations-panel",
        "fuzz",
        "miri",
    ] {
        for artifact in result_artifacts(task).unwrap() {
            if let Some(name) = artifact.strip_suffix(".json") {
                assert!(
                    xtask::evidence::receipt_scope(name).is_ok(),
                    "missing scope for {name}"
                );
            }
        }
    }
    assert!(xtask::evidence::receipt_scope("bounded-fuzz")
        .unwrap()
        .contains(&"xtask/src/lib.rs"));
    assert!(xtask::evidence::receipt_scope("unknown").is_err());
    assert!(result_artifacts("miri-runtime-boundaries")
        .unwrap()
        .contains(&"miri-runtime-packets.json"));
}

#[test]
fn core_shards_cover_both_halves_with_the_same_fixed_denominator() {
    let shards = xtask::evidence::CORE_MUTATION_SHARDS;
    assert_eq!(
        shards
            .iter()
            .map(|shard| shard.selector)
            .collect::<Vec<_>>(),
        ["0/2", "1/2"]
    );
    for shard in shards {
        assert!(result_artifacts(shard.task)
            .unwrap()
            .contains(&format!("{}.json", shard.receipt).as_str()));
        assert_eq!(
            xtask::evidence::receipt_scope(shard.receipt).unwrap(),
            ["crates/device-core"]
        );
    }
}

#[test]
fn interpreter_cases_have_separate_artifacts_without_dropping_the_replay_suite() {
    let adapters = result_artifacts("miri-adapters").expect("separate adapter work");
    assert!(adapters.contains(&"miri-inventory.json"));
    let mut expected = adapters.to_vec();
    for name in [
        "miri-replay-atomicity",
        "miri-display-hold",
        "miri-repeat-hold",
        "miri-timeout-mode",
        "miri-command-atomicity",
        "miri-replay-lines",
    ] {
        let artifacts = result_artifacts(name).expect("separate replay case");
        assert_eq!(artifacts, [format!("{name}.json")]);
        expected.extend(artifacts);
    }
    expected.sort();
    let mut aggregate = result_artifacts("miri").unwrap().to_vec();
    aggregate.sort();
    assert_eq!(aggregate, expected);
    let ble = xtask::evidence::MIRI_BLE_CASES;
    assert_eq!(ble.len(), 3);
    let mut names = std::collections::BTreeSet::new();
    let mut artifacts = std::collections::BTreeSet::new();
    for case in ble {
        assert!(names.insert(case.filter));
        assert!(xtask::runtime_test_inventory::CASES.contains(&case.filter));
        assert_eq!(result_artifacts(case.task).unwrap(), case.artifacts);
        for artifact in case.artifacts {
            assert!(artifacts.insert(artifact));
        }
    }
}

#[test]
fn interpreter_inventory_rejects_empty_missing_duplicate_and_unexpected_cases() {
    use xtask::evidence::verify_test_inventory;
    assert!(
        verify_test_inventory("a: test\nb: test\n\n2 tests, 0 benchmarks", &["b", "a"]).is_ok()
    );
    for invalid in [
        "",
        "a: test",
        "a: test\na: test\nb: test",
        "a: test\nb: test\nc: test",
        "a: ignored\nb: test",
    ] {
        assert!(
            verify_test_inventory(invalid, &["a", "b"]).is_err(),
            "{invalid}"
        );
    }
    assert!(verify_test_inventory("", &[]).is_err());
    assert!(verify_test_inventory("a: test", &["a", "a"]).is_err());
    let cases = xtask::evidence::MIRI_PREVIEW_CASES;
    let report = cases
        .iter()
        .map(|case| format!("{}: test\n", case.filter))
        .collect::<String>();
    let names = cases.iter().map(|case| case.filter).collect::<Vec<_>>();
    assert!(verify_test_inventory(&report, &names).is_ok());
    assert_eq!(cases.len(), 6);
}
use xtask::evidence::{result_artifacts, validate_mutations};

fn report() -> Value {
    json!({"total_mutants":1,"caught":1,"unviable":0,"missed":0,"timeout":0,"success":0,"cargo_mutants_version":"27.1.0","outcomes":[{"scenario":"Baseline","summary":"Success"},{"scenario":{"Mutant":{"name":"broken behavior","file":"src/app.rs"}},"summary":"CaughtMutant"}]})
}
#[test]
fn mutation_evidence_requires_real_complete_work_for_every_selected_file() {
    assert_eq!(
        validate_mutations(&report().to_string(), "27.1.0", &["src/app.rs"]),
        Ok(1)
    );
    assert!(validate_mutations(&report().to_string(), "27.1.0", &["src/missing.rs"]).is_err());
    assert!(validate_mutations(&report().to_string(), "27.1.0", &[]).is_err());
    for field in [
        "total_mutants",
        "caught",
        "missed",
        "timeout",
        "success",
        "unviable",
    ] {
        let mut invalid = report();
        invalid[field] = json!(9);
        assert!(
            validate_mutations(&invalid.to_string(), "27.1.0", &["src/app.rs"]).is_err(),
            "{field}"
        );
    }
    for outcome in ["MissedMutant", "Timeout", "Failure", "Success"] {
        let mut invalid = report();
        invalid["outcomes"][1]["summary"] = json!(outcome);
        assert!(validate_mutations(&invalid.to_string(), "27.1.0", &["src/app.rs"]).is_err());
    }
    for invalid in [json!({}), json!({"outcomes":[]}), json!("not a report")] {
        assert!(validate_mutations(&invalid.to_string(), "27.1.0", &["src/app.rs"]).is_err());
    }
    let mut invalid = report();
    invalid["outcomes"][0]["summary"] = json!("Failure");
    assert!(validate_mutations(&invalid.to_string(), "27.1.0", &["src/app.rs"]).is_err());
    assert!(validate_mutations(&report().to_string(), "wrong-version", &["src/app.rs"]).is_err());
}

#[test]
fn independent_verification_tasks_cannot_overwrite_each_others_results() {
    let tasks = [
        "proofs-runtime",
        "miri-runtime-boundaries",
        "miri-runtime-ble",
        "miri-runtime-ble-restart",
        "miri-runtime-ble-cached",
        "mutations-runtime-shard-0",
        "mutations-runtime-shard-1",
        "mutations-runtime-shard-2",
        "mutations-runtime-shard-3",
        "proofs",
        "proofs-preview",
        "proofs-tools",
        "mutations",
        "mutations-preview",
        "mutations-panel",
        "fuzz",
        "miri",
    ];
    let mut owned = std::collections::BTreeSet::new();
    for task in tasks {
        let artifacts = result_artifacts(task).unwrap();
        assert!(!artifacts.is_empty());
        for artifact in artifacts {
            assert!(owned.insert(*artifact), "duplicate artifact {artifact}");
            assert!(!artifact.contains('/') && !artifact.contains(".."));
        }
    }
    assert!(result_artifacts("unknown").is_err());
}
