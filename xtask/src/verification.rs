use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

type VerifyResult = Result<(), Box<dyn Error + Send + Sync>>;
const SHA256_HEX_BYTES: usize = 64;
const MUTATION_OUTPUT_DIRECTORY: &str = "/tmp";
const MUTATION_RESULT_DIRECTORY: &str = "/tmp/mutants.out";
const FUZZ_WORKING_CORPUS: &str = "/tmp/e1004-corpus";
const CONTAINER_WORK_DIRECTORY: &str = "/work";
const TIMEOUT_EXIT: i32 = 124;
const RESOURCE_KILL_EXIT: i32 = 137;
const RECEIPT_FORMAT: u32 = 3;
const CONTAINER_IMAGE_VARIABLE: &str = "E1004_VERIFICATION_CONTAINER_IMAGE";
const SNAPSHOT_DIRECTORY: &str = "/tmp/e1004-source";
const PROGRESS_CHUNK_BYTES: usize = 4096;
const PREVIEW_HARNESSES: &[&str] = &[
    "proofs::recording_budget_matches_wide_arithmetic_at_every_admitted_boundary",
    "proofs::a_cancelled_preview_update_cannot_be_presented_by_mode_switch_or_late_success",
];
const TOOL_HARNESSES: &[&str] =
    &["evidence::proofs::mutation_verdict_requires_nonempty_complete_counts_without_overflow"];
const RUNTIME_HARNESSES: &[&str] = &[
    "proofs::sleep_permission_requires_current_ticket_and_every_quiet_boundary",
    "proofs::an_emitted_sleep_action_passes_the_request_boundary",
    "proofs::display_requests_match_only_the_active_generation",
    "proofs::erased_settings_are_defaults_and_non_erased_corruption_is_reported",
    "proofs::charger_status_reads_only_the_two_documented_bits",
    "proofs::boot_recovers_with_both_slots_readable",
    "proofs::boot_recovers_with_first_slot_unreadable",
    "proofs::boot_recovers_with_second_slot_unreadable",
    "proofs::boot_reports_failure_with_both_slots_unreadable",
    "proofs::packet_reservations_never_exceed_capacity_or_release_an_invalid_count",
];

#[derive(Clone, Deserialize, Serialize)]
struct Receipt {
    version: u32,
    command: Vec<String>,
    source_sha256: String,
    artifact_sha256: Option<String>,
    container_image: Option<String>,
    exit: Option<i32>,
    stdout: String,
    stderr: String,
}

#[derive(Deserialize)]
struct Mount {
    #[serde(rename = "Type")]
    kind: String,
    #[serde(rename = "Source")]
    source: PathBuf,
    #[serde(rename = "Destination")]
    destination: PathBuf,
}

fn current(name: &str, success: bool) -> Result<String, Box<dyn Error + Send + Sync>> {
    let receipt: Receipt =
        serde_json::from_slice(&fs::read(format!("verification/results/{name}.json"))?)?;
    if receipt.version != RECEIPT_FORMAT
        || receipt.source_sha256 != identity(name, xtask::evidence::receipt_scope(name)?)?
        || receipt.container_image.as_deref() != Some(configured_image(name)?.as_str())
    {
        return Err(format!("stale verification result: {name}").into());
    }
    if receipt.exit.is_none() || (receipt.exit == Some(0)) != success {
        return Err(format!("unsuccessful verification result: {name}").into());
    }
    if name.starts_with("core-mutations-")
        || name.starts_with("runtime-mutations-")
        || matches!(name, "preview-mutations" | "panel-mutations")
    {
        let bytes = fs::read(format!("verification/results/{name}/outcomes.json"))?;
        if receipt.artifact_sha256.as_deref()
            != Some(format!("{:x}", Sha256::digest(bytes)).as_str())
        {
            return Err("mutation artifact does not match its receipt".into());
        }
    }
    Ok(format!("{}\n{}", receipt.stdout, receipt.stderr))
}

pub fn check_results() -> VerifyResult {
    let config = inventory()?;
    let core = current("core-proofs", true)?;
    let bsp = current("bsp-proofs", true)?;
    let ui = current("ui-proofs", true)?;
    for (report, field) in [
        (&core, "core_harnesses"),
        (&bsp, "bsp_harnesses"),
        (&ui, "ui_harnesses"),
    ] {
        let names = config[field]
            .as_array()
            .ok_or("missing harness inventory")?;
        if names.is_empty()
            || !report.contains(&format!(
                "{} successfully verified harnesses, 0 failures, {} total",
                names.len(),
                names.len()
            ))
        {
            return Err("incomplete proof receipt".into());
        }
        for name in names {
            if !report.contains(&format!(
                "Checking harness proofs::{}",
                name.as_str().ok_or("invalid harness")?
            )) {
                return Err("missing checked harness".into());
            }
        }
    }
    let negative = current("negative-control", false)?;
    if !negative.contains("VERIFICATION:- FAILED") || !negative.contains("assertion failed") {
        return Err("false-claim control did not reject its assertion".into());
    }
    for (name, harnesses) in [
        ("preview-proofs", PREVIEW_HARNESSES),
        ("tool-proofs", TOOL_HARNESSES),
        ("runtime-proofs", RUNTIME_HARNESSES),
    ] {
        verify_proof_report(&current(name, true)?, harnesses)?;
    }
    let mutations = current("core-mutations-0", true)?;
    if !mutations.contains("mutants tested")
        || mutations.contains("MISSED")
        || mutations.contains("TIMEOUT")
    {
        return Err("unresolved mutation result".into());
    }
    for (name, selection) in [
        ("core-mutations-0", "mutation_files"),
        ("core-mutations-1", "mutation_files"),
        ("preview-mutations", "preview_mutation_files"),
        ("panel-mutations", "panel_mutation_files"),
        ("runtime-mutations-0", "runtime_mutation_files"),
        ("runtime-mutations-1", "runtime_mutation_files"),
        ("runtime-mutations-2", "runtime_mutation_files"),
        ("runtime-mutations-3", "runtime_mutation_files"),
    ] {
        current(name, true)?;
        let files = config[selection]
            .as_array()
            .ok_or("missing mutation selection")?
            .iter()
            .map(|file| file.as_str().ok_or("invalid mutation path"))
            .collect::<Result<Vec<_>, _>>()?;
        let count = xtask::evidence::validate_mutations(
            &fs::read_to_string(format!("verification/results/{name}/outcomes.json"))?,
            config["mutants"]
                .as_str()
                .ok_or("missing mutation version")?,
            &files,
        )?;
        println!("Mutation suite {name}: {count} outcomes, no survivors or timeouts");
    }
    let fuzz = current("bounded-fuzz", true)?;
    if !fuzz.contains("10000 runs") {
        return Err("incomplete fuzz receipt".into());
    }
    let miri_bsp = current("miri-bsp", true)?;
    let miri_panel = current("miri-panel", true)?;
    let miri_ui = current("miri-ui", true)?;
    if !miri_bsp.contains("6 passed")
        || !miri_panel.contains("1 passed")
        || !miri_ui.contains("1 passed")
    {
        return Err("empty or incomplete Miri receipt".into());
    }
    let cases = xtask::evidence::MIRI_PREVIEW_CASES;
    xtask::evidence::verify_test_inventory(
        &current("miri-inventory", true)?,
        &cases.iter().map(|case| case.filter).collect::<Vec<_>>(),
    )?;
    for case in cases {
        verify_interpreter_case(&current(case.task, true)?, case.filter)?;
    }
    for (suite, names) in [
        (
            "measurements",
            &["production_measurement_reads_the_documented_registers_and_turns_the_divider_off"][..],
        ),
        (
            "policy",
            &[
                "a_sleep_ticket_never_allows_an_active_radio_or_a_stale_request",
                "every_busy_boundary_and_every_stale_ticket_blocks_sleep",
                "settings_boot_distinguishes_erased_corrupt_and_valid_generations",
            ][..],
        ),
        (
            "packets",
            &["packet_pool::owned::tests::packets_own_disjoint_buffers_and_release_the_complete_bounded_capacity"][..],
        ),
    ] {
        let report = current(&format!("miri-runtime-{suite}"), true)?;
        for name in names {
            if !report.contains(&format!("test {name} ... ok")) {
                return Err("missing runtime interpreter case".into());
            }
        }
    }
    for case in xtask::evidence::MIRI_BLE_CASES {
        verify_interpreter_case(&current(case.task, true)?, case.filter)?;
    }
    println!("Current source identities match: 32 Kani harnesses, false-claim rejection, four mutation suites with no survivors or timeouts, 10000 fuzz inputs, and 22 Miri tests");
    println!(
        "Mutation summary: {}",
        mutations
            .lines()
            .find(|line| line.contains("mutants tested"))
            .ok_or("missing mutation summary")?
    );
    Ok(())
}

fn verify_proof_report(report: &str, harnesses: &[&str]) -> VerifyResult {
    if harnesses.is_empty()
        || !report.contains(&format!(
            "{} successfully verified harnesses, 0 failures, {} total",
            harnesses.len(),
            harnesses.len()
        ))
        || !harnesses
            .iter()
            .all(|harness| report.contains(&format!("Checking harness {harness}")))
    {
        return Err("incomplete additional proof report".into());
    }
    Ok(())
}

fn verify_interpreter_case(report: &str, name: &str) -> VerifyResult {
    if !report.contains(&format!("test {name} ... ok"))
        || !report.contains("1 passed; 0 failed; 0 ignored")
    {
        return Err("interpreter case was skipped or incomplete".into());
    }
    Ok(())
}

fn extra_proofs(name: &str, package: &str, harnesses: &[&str]) -> VerifyResult {
    let report = capture(
        name,
        &[
            "kani",
            "-p",
            package,
            "--lib",
            "--jobs",
            "1",
            "--output-format",
            "terse",
        ],
        true,
    )?;
    verify_proof_report(&report, harnesses)
}

fn files(path: &Path, result: &mut Vec<PathBuf>) -> VerifyResult {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err("verification inputs must not be symbolic links".into());
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            files(&entry?.path(), result)?;
        }
    } else {
        result.push(path.to_owned());
    }
    Ok(())
}

fn identity(name: &str, paths: &[&str]) -> Result<String, Box<dyn Error + Send + Sync>> {
    let mut inventory = Vec::new();
    for path in xtask::evidence::verification_inputs(paths) {
        files(Path::new(path), &mut inventory)?;
    }
    inventory.sort();
    inventory.dedup();
    let mut hash = Sha256::new();
    hash.update(name.as_bytes());
    hash.update([0]);
    for path in inventory {
        hash.update(path.as_os_str().as_encoded_bytes());
        hash.update([0]);
        hash.update(fs::read(path)?);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn input_hashes(
    root: &Path,
    paths: &[&str],
) -> Result<BTreeMap<PathBuf, String>, Box<dyn Error + Send + Sync>> {
    let mut inventory = Vec::new();
    for path in xtask::evidence::verification_inputs(paths) {
        files(&root.join(path), &mut inventory)?;
    }
    inventory
        .into_iter()
        .map(|path| {
            Ok((
                path.strip_prefix(root)?.to_owned(),
                format!("{:x}", Sha256::digest(fs::read(path)?)),
            ))
        })
        .collect()
}

fn changed_inputs(before: &BTreeMap<PathBuf, String>, after: &BTreeMap<PathBuf, String>) -> String {
    before
        .keys()
        .chain(after.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|path| before.get(*path) != after.get(*path))
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn mirror_output(mut input: impl Read, mut mirror: impl Write) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut chunk = [0; PROGRESS_CHUNK_BYTES];
    loop {
        let count = match input.read(&mut chunk) {
            Ok(0) => return Ok(bytes),
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        mirror.write_all(&chunk[..count])?;
        bytes.extend_from_slice(&chunk[..count]);
    }
}

fn progress_output(command: &mut Command) -> Result<Output, Box<dyn Error + Send + Sync>> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().ok_or("missing verification stdout")?;
    let stderr = child.stderr.take().ok_or("missing verification stderr")?;
    std::thread::scope(|scope| {
        let out = scope.spawn(|| mirror_output(stdout, std::io::stdout()));
        let err = scope.spawn(|| mirror_output(stderr, std::io::stderr()));
        Ok(Output {
            status: child.wait()?,
            stdout: out
                .join()
                .map_err(|_| "verification stdout reader panicked")??,
            stderr: err
                .join()
                .map_err(|_| "verification stderr reader panicked")??,
        })
    })
}

fn capture(
    name: &str,
    args: &[&str],
    expected_success: bool,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let scope = xtask::evidence::receipt_scope(name)?;
    let inputs = input_hashes(Path::new("."), scope)?;
    let before = identity(name, scope)?;
    println!("phase=verification name={name} args={args:?} source_sha256={before}");
    let mut command = xtask::evidence::verification_command(args);
    let output = if name.starts_with("miri-runtime-ble") {
        progress_output(&mut command)?
    } else {
        command.output()?
    };
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    fs::create_dir_all("verification/results")?;
    let receipt = Receipt {
        version: RECEIPT_FORMAT,
        command: std::iter::once("cargo".to_owned())
            .chain(args.iter().map(|s| (*s).to_owned()))
            .collect(),
        source_sha256: before.clone(),
        artifact_sha256: if args.first() == Some(&"mutants")
            && !args.contains(&"--version")
            && !args.contains(&"--list")
        {
            match fs::read(Path::new(MUTATION_RESULT_DIRECTORY).join("outcomes.json")) {
                Ok(bytes) => Some(format!("{:x}", Sha256::digest(bytes))),
                Err(error) => {
                    eprintln!("mutation artifact unavailable: {error}");
                    None
                }
            }
        } else {
            None
        },
        container_image: Some(
            std::env::var(CONTAINER_IMAGE_VARIABLE)
                .map_err(|_| "verification must run through its bounded container entrypoint")?,
        ),
        exit: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    };
    publish_receipt(name, &inputs, &receipt)?;
    if output.status.success() != expected_success {
        eprintln!("{text}");
        return Err(format!("{name} returned {}", output.status).into());
    }
    Ok(text)
}

fn publish_receipt(
    name: &str,
    inputs: &BTreeMap<PathBuf, String>,
    receipt: &Receipt,
) -> VerifyResult {
    let mut receipt = receipt.clone();
    let validation: VerifyResult = (|| {
        let scope = xtask::evidence::receipt_scope(name)?;
        let after = input_hashes(Path::new("."), scope)?;
        if &after != inputs || identity(name, scope)? != receipt.source_sha256 {
            return Err(format!(
                "source changed during {name}: {}",
                changed_inputs(inputs, &after)
            )
            .into());
        }
        Ok(())
    })();
    if let Err(error) = &validation {
        receipt.exit = None;
        receipt.stderr.push_str(&format!("\n{error}\n"));
    }
    fs::create_dir_all("verification/results")?;
    fs::write(
        format!("verification/results/{name}.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    validation
}

fn inventory() -> Result<toml::Value, Box<dyn Error + Send + Sync>> {
    Ok(toml::from_str(&fs::read_to_string(
        "verification/inventory.toml",
    )?)?)
}

fn proofs() -> VerifyResult {
    let config = inventory()?;
    let version = capture("kani-version", &["kani", "--version"], true)?;
    for field in ["kani", "cbmc"] {
        if !version.contains(config[field].as_str().ok_or("missing verifier version")?) {
            return Err(format!("unexpected {field} version").into());
        }
    }
    let report = capture(
        "core-proofs",
        &[
            "kani",
            "-p",
            "device-core",
            "--jobs",
            "1",
            "--output-format",
            "terse",
        ],
        true,
    )?;
    let names = config["core_harnesses"]
        .as_array()
        .ok_or("missing harness inventory")?;
    if names.is_empty() {
        return Err("empty proof inventory".into());
    }
    for name in names {
        if !report.contains(&format!(
            "Checking harness proofs::{}",
            name.as_str().ok_or("invalid harness name")?
        )) {
            return Err("a required harness did not run".into());
        }
    }
    if !report.contains(&format!(
        "{} successfully verified harnesses, 0 failures, {} total",
        names.len(),
        names.len()
    )) {
        return Err("incomplete proof summary".into());
    }
    bsp_proofs(&config)?;
    ui_proofs(&config)?;
    let negative = capture(
        "negative-control",
        &[
            "kani",
            "--manifest-path",
            "verification/negative/Cargo.toml",
            "--output-format",
            "terse",
        ],
        false,
    )?;
    if !negative.contains("VERIFICATION:- FAILED")
        || !negative.contains("rejects_a_false_contract")
        || !negative.contains("assertion failed")
    {
        return Err("negative control did not reject the asserted contract".into());
    }
    println!("Source-bound proofs and false-contract rejection passed");
    Ok(())
}

fn bsp_proofs(config: &toml::Value) -> VerifyResult {
    let bsp = capture(
        "bsp-proofs",
        &[
            "kani",
            "-p",
            "e1004-bsp",
            "--jobs",
            "1",
            "--output-format",
            "terse",
        ],
        true,
    )?;
    let bsp_names = config["bsp_harnesses"]
        .as_array()
        .ok_or("missing BSP harness inventory")?;
    for name in bsp_names {
        if !bsp.contains(&format!(
            "Checking harness proofs::{}",
            name.as_str().ok_or("invalid BSP harness")?
        )) {
            return Err("a BSP harness did not run".into());
        }
    }
    if !bsp.contains(&format!(
        "{} successfully verified harnesses, 0 failures, {} total",
        bsp_names.len(),
        bsp_names.len()
    )) {
        return Err("incomplete BSP proof summary".into());
    }
    println!("Board source-bound proofs passed");
    Ok(())
}

fn ui_proofs(config: &toml::Value) -> VerifyResult {
    let report = capture(
        "ui-proofs",
        &[
            "kani",
            "-p",
            "device-ui",
            "--jobs",
            "1",
            "--output-format",
            "terse",
        ],
        true,
    )?;
    let names = config["ui_harnesses"]
        .as_array()
        .ok_or("missing UI harnesses")?;
    if names.is_empty()
        || !report.contains(&format!(
            "{} successfully verified harnesses, 0 failures, {} total",
            names.len(),
            names.len()
        ))
    {
        return Err("incomplete UI proof summary".into());
    }
    for name in names {
        if !report.contains(&format!(
            "Checking harness proofs::{}",
            name.as_str().ok_or("invalid UI harness")?
        )) {
            return Err("a UI harness did not run".into());
        }
    }
    println!("UI source-bound proof passed");
    Ok(())
}

fn mutations(package: &str, selection: &str, name: &str, shard: Option<&str>) -> VerifyResult {
    let config = inventory()?;
    let version = capture("mutants-version", &["mutants", "--version"], true)?;
    if !version.contains(
        config["mutants"]
            .as_str()
            .ok_or("missing mutation version")?,
    ) {
        return Err("unexpected cargo-mutants version".into());
    }
    let files = config[selection]
        .as_array()
        .ok_or("missing mutation file selection")?;
    if files.is_empty() {
        return Err("empty mutation selection".into());
    }
    let mut args = vec![
        "mutants",
        "--package",
        package,
        "--jobs",
        "2",
        "--no-shuffle",
        "--timeout",
        "60",
        "--output",
        MUTATION_OUTPUT_DIRECTORY,
    ];
    if package == "device-runtime" {
        args.push("--all-features");
    }
    if let Some(shard) = shard {
        args.extend(["--shard", shard, "--sharding", "round-robin"]);
    }
    for file in files {
        args.push("--file");
        args.push(file.as_str().ok_or("invalid mutation file")?);
    }
    let result = capture(name, &args, true);
    if Path::new(MUTATION_RESULT_DIRECTORY).exists() {
        copy_tree(
            Path::new(MUTATION_RESULT_DIRECTORY),
            &Path::new("verification/results").join(name),
        )?;
    }
    let report = result?;
    if !report.contains("mutants tested") || report.contains("MISSED") || report.contains("TIMEOUT")
    {
        return Err("mutation report has unresolved outcomes".into());
    }
    let files = files
        .iter()
        .map(|file| file.as_str().ok_or("invalid mutation path"))
        .collect::<Result<Vec<_>, _>>()?;
    xtask::evidence::validate_mutations(
        &fs::read_to_string(format!("verification/results/{name}/outcomes.json"))?,
        config["mutants"]
            .as_str()
            .ok_or("missing mutation version")?,
        &files,
    )?;
    println!("Declared mutation selection passed");
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path) -> VerifyResult {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn copy_workspace(source: &Path, destination: &Path) -> VerifyResult {
    fs::create_dir(destination)?;
    copy_workspace_directory(source, destination, Path::new(""))
}

fn copy_workspace_directory(source: &Path, destination: &Path, relative: &Path) -> VerifyResult {
    for entry in fs::read_dir(source.join(relative))? {
        let entry = entry?;
        let name = entry.file_name();
        let path = relative.join(&name);
        if name == ".git"
            || name == "target"
            || name == "node_modules"
            || path == Path::new("verification/results")
        {
            continue;
        }
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(format!(
                "verification source must not be a symbolic link: {}",
                path.display()
            )
            .into());
        }
        if kind.is_dir() {
            fs::create_dir(destination.join(&path))?;
            copy_workspace_directory(source, destination, &path)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), destination.join(path))?;
        } else {
            return Err("verification source must be a regular file or directory".into());
        }
    }
    Ok(())
}

fn capture_workspace(source: &Path, destination: &Path, task: &str) -> VerifyResult {
    let mut paths = Vec::new();
    for artifact in xtask::evidence::result_artifacts(task)? {
        if let Some(name) = artifact.strip_suffix(".json") {
            paths.extend(xtask::evidence::receipt_scope(name)?);
        }
    }
    let before = input_hashes(source, &paths)?;
    copy_workspace(source, destination)?;
    for root in [source, destination] {
        let after = input_hashes(root, &paths)?;
        if before != after {
            return Err(format!(
                "verification inputs changed during capture: {}",
                changed_inputs(&before, &after)
            )
            .into());
        }
    }
    Ok(())
}

pub fn snapshot(task: &str) -> VerifyResult {
    let source = std::env::current_dir()?;
    let destination = Path::new(SNAPSHOT_DIRECTORY);
    capture_workspace(&source, destination, task)?;
    std::env::set_current_dir(destination)?;
    let result = inside(task);
    std::env::set_current_dir(&source)?;
    fs::create_dir_all(source.join("verification/results"))?;
    let mut transfer = Ok(());
    for artifact in xtask::evidence::result_artifacts(task)? {
        let captured = destination.join("verification/results").join(artifact);
        let output = source.join("verification/results").join(artifact);
        if captured.try_exists()? {
            let copied = if captured.is_dir() {
                copy_tree(&captured, &output)
            } else {
                fs::copy(&captured, &output).map(|_| ()).map_err(Into::into)
            };
            if let Err(error) = copied {
                eprintln!("verification artifact transfer failed for {artifact}: {error}");
                transfer = Err(error);
            }
        }
    }
    result.and(transfer)
}

pub fn inside(task: &str) -> VerifyResult {
    if let Some(case) = xtask::evidence::MIRI_BLE_CASES
        .iter()
        .find(|case| case.task == task)
    {
        capture(
            case.task,
            &[
                "+nightly-2026-08-21",
                "miri",
                "test",
                "--locked",
                "-p",
                "device-runtime",
                "--features",
                "ble",
                "--target",
                "x86_64-unknown-linux-gnu",
                "--test",
                "e2e",
                case.filter,
                "--",
                "--exact",
                "--nocapture",
            ],
            true,
        )?;
        return verify_interpreter_case(&current(case.task, true)?, case.filter);
    }
    if let Some(shard) = xtask::evidence::RUNTIME_MUTATION_SHARDS
        .iter()
        .find(|shard| shard.task == task)
    {
        return mutations(
            "device-runtime",
            "runtime_mutation_files",
            shard.receipt,
            Some(shard.selector),
        );
    }
    if let Some(case) = xtask::evidence::MIRI_PREVIEW_CASES
        .iter()
        .find(|case| case.task == task)
    {
        let report = capture(
            case.task,
            &[
                "+nightly-2026-08-21",
                "miri",
                "test",
                "--locked",
                "--target",
                "x86_64-unknown-linux-gnu",
                "-p",
                "device-preview",
                "--test",
                "replay",
                case.filter,
                "--",
                "--exact",
            ],
            true,
        )?;
        verify_interpreter_case(&report, case.filter)?;
        println!("Miri interpreted {}", case.filter);
        return Ok(());
    }
    if let Some(shard) = xtask::evidence::CORE_MUTATION_SHARDS
        .iter()
        .find(|shard| shard.task == task)
    {
        return mutations(
            "device-core",
            "mutation_files",
            shard.receipt,
            Some(shard.selector),
        );
    }
    match task {
        "proofs-runtime" => extra_proofs("runtime-proofs", "device-runtime", RUNTIME_HARNESSES),
        "runtime-mutation-inventory" => {
            let report = capture(
                "runtime-mutation-inventory",
                &[
                    "mutants",
                    "--package",
                    "device-runtime",
                    "--all-features",
                    "--list",
                    "--json",
                ],
                true,
            )?;
            let list: serde_json::Value = serde_json::from_str(report.trim())?;
            println!(
                "Runtime mutation inventory: {} candidates",
                list.as_array().ok_or("invalid mutation inventory")?.len()
            );
            Ok(())
        }
        "miri-runtime-boundaries" => {
            for suite in ["measurements", "policy"] {
                capture(
                    &format!("miri-runtime-{suite}"),
                    &[
                        "+nightly-2026-08-21",
                        "miri",
                        "test",
                        "--locked",
                        "--target",
                        "x86_64-unknown-linux-gnu",
                        "-p",
                        "device-runtime",
                        "--test",
                        suite,
                    ],
                    true,
                )?;
            }
            capture(
                "miri-runtime-packets",
                &[
                    "+nightly-2026-08-21",
                    "miri",
                    "test",
                    "--locked",
                    "--target",
                    "x86_64-unknown-linux-gnu",
                    "-p",
                    "device-runtime",
                    "--features",
                    "ble",
                    "--lib",
                    "packet_pool::owned::tests::packets_own_disjoint_buffers_and_release_the_complete_bounded_capacity",
                    "--",
                    "--exact",
                ],
                true,
            )?;
            Ok(())
        }
        "proofs" => proofs(),
        "proofs-bsp" => bsp_proofs(&inventory()?),
        "proofs-ui" => ui_proofs(&inventory()?),
        "proofs-preview" => extra_proofs("preview-proofs", "device-preview", PREVIEW_HARNESSES),
        "proofs-tools" => extra_proofs("tool-proofs", "xtask", TOOL_HARNESSES),
        "mutations-preview" => mutations(
            "device-preview",
            "preview_mutation_files",
            "preview-mutations",
            None,
        ),
        "mutations-panel" => mutations("t133a01", "panel_mutation_files", "panel-mutations", None),
        "fuzz" => {
            let corpus = Path::new("fuzz/corpus/boundaries");
            if fs::read_dir(corpus)?.count() == 0 {
                return Err("empty committed fuzz corpus".into());
            }
            copy_tree(corpus, Path::new(FUZZ_WORKING_CORPUS))?;
            let report = capture(
                "bounded-fuzz",
                &[
                    "+nightly-2026-08-21",
                    "fuzz",
                    "run",
                    "boundaries",
                    FUZZ_WORKING_CORPUS,
                    "--fuzz-dir",
                    "fuzz",
                    "--",
                    "-runs=10000",
                    "-seed=104",
                    "-max_len=8192",
                    "-rss_limit_mb=1024",
                ],
                true,
            )?;
            if !report.contains("10000 runs") {
                return Err("fuzz input count did not complete".into());
            }
            println!("Fixed-seed fuzz campaign completed 10000 runs");
            Ok(())
        }
        "miri-adapters" => {
            let report = capture(
                "miri-inventory",
                &[
                    "test",
                    "--locked",
                    "-p",
                    "device-preview",
                    "--test",
                    "replay",
                    "--",
                    "--list",
                ],
                true,
            )?;
            xtask::evidence::verify_test_inventory(
                &report,
                &xtask::evidence::MIRI_PREVIEW_CASES
                    .iter()
                    .map(|case| case.filter)
                    .collect::<Vec<_>>(),
            )?;
            capture(
                "miri-bsp",
                &[
                    "+nightly-2026-08-21",
                    "miri",
                    "test",
                    "--locked",
                    "--target",
                    "x86_64-unknown-linux-gnu",
                    "-p",
                    "e1004-bsp",
                    "--test",
                    "boundaries",
                ],
                true,
            )?;
            capture(
                "miri-panel",
                &[
                    "+nightly-2026-08-21",
                    "miri",
                    "test",
                    "--locked",
                    "--target",
                    "x86_64-unknown-linux-gnu",
                    "-p",
                    "t133a01",
                    "--test",
                    "protocol",
                    "timeout_and_transport_errors_stop_the_protocol_and_release_the_bus",
                ],
                true,
            )?;
            capture(
                "miri-ui",
                &[
                    "+nightly-2026-08-21",
                    "miri",
                    "test",
                    "--locked",
                    "--target",
                    "x86_64-unknown-linux-gnu",
                    "-p",
                    "device-ui",
                    "--test",
                    "rendering",
                    "text_capacity_failure_replaces_partial_content_with_a_visible_error",
                ],
                true,
            )?;
            println!(
                "Miri interpreted BSP boundaries, panel failure cleanup, and UI text overflow"
            );
            Ok(())
        }
        _ => Err("unknown container verification task".into()),
    }
}

fn invalidate_task_results(task: &str) -> VerifyResult {
    for artifact in xtask::evidence::result_artifacts(task)? {
        if artifact.ends_with(".json") {
            let path = Path::new("verification/results").join(artifact);
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
    Ok(())
}

fn configured_image(task: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
    let (variable, identity) = if task.starts_with("miri")
        || task.starts_with("mutations")
        || task.contains("-mutations")
        || matches!(
            task,
            "fuzz" | "bounded-fuzz" | "runtime-mutation-inventory" | "mutants-version"
        ) {
        (
            "E1004_HOST_VERIFICATION_IMAGE",
            "target/host-verification-image.id",
        )
    } else {
        ("E1004_VERIFICATION_IMAGE", "target/verification-image.id")
    };
    let image = if let Ok(image) = std::env::var(CONTAINER_IMAGE_VARIABLE) {
        image
    } else {
        match std::env::var(variable) {
            Ok(image) => image,
            Err(_) => fs::read_to_string(identity).map_err(|_| format!("run mise x -- just verification-image or set {variable} to an immutable image identity"))?.trim().to_owned(),
        }
    };
    if !image.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == SHA256_HEX_BYTES && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    }) {
        return Err("verification image must be an immutable sha256 identity".into());
    }
    Ok(image)
}

pub fn bounded(task: &str) -> VerifyResult {
    if task == "miri-runtime" {
        bounded("miri-runtime-boundaries")?;
        for case in xtask::evidence::MIRI_BLE_CASES {
            bounded(case.task)?;
        }
        return Ok(());
    }
    if task == "miri" {
        bounded("miri-runtime")?;
        bounded("miri-adapters")?;
        const INTERPRETER_WORKERS: usize = 2;
        for batch in xtask::evidence::MIRI_PREVIEW_CASES.chunks(INTERPRETER_WORKERS) {
            let results = std::thread::scope(|scope| {
                let workers = batch
                    .iter()
                    .map(|case| scope.spawn(move || bounded(case.task)))
                    .collect::<Vec<_>>();
                workers
                    .into_iter()
                    .map(|worker| worker.join())
                    .collect::<Vec<_>>()
            });
            for result in results {
                result.map_err(|_| "interpreter worker panicked")??;
            }
        }
        return Ok(());
    }
    if task == "mutations" {
        for shard in xtask::evidence::CORE_MUTATION_SHARDS {
            bounded(shard.task)?;
        }
        return Ok(());
    }
    if task == "proofs-all" {
        for suite in ["proofs", "proofs-preview", "proofs-tools", "proofs-runtime"] {
            bounded(suite)?;
        }
        return Ok(());
    }
    if task == "mutations-all" {
        for suite in ["mutations", "mutations-preview", "mutations-panel"] {
            bounded(suite)?;
        }
        for shard in xtask::evidence::RUNTIME_MUTATION_SHARDS {
            bounded(shard.task)?;
        }
        return Ok(());
    }
    let image = configured_image(task)?;
    let image_environment = format!("{CONTAINER_IMAGE_VARIABLE}={image}");
    let owner_policy = std::env::var_os("HOME")
        .map(PathBuf::from)
        .is_some_and(|home| home.join(".config/dotfiles/bounded-tools.toml").is_file());
    let mut runner = if owner_policy {
        Command::new("dotfiles")
    } else {
        Command::new("docker")
    };
    if owner_policy {
        runner.args([
            "bounded",
            "run",
            "--keep",
            "--image",
            &image,
            "--project",
            ".",
            "--",
            "env",
            "CARGO_TARGET_DIR=/tmp/e1004-xtask",
            "CARGO_INCREMENTAL=0",
            &image_environment,
            "cargo",
            "run",
            "--locked",
            "-p",
            "xtask",
            "--",
            "verify-container",
            task,
        ]);
    } else {
        let project = std::env::current_dir()?;
        let mount = format!("type=bind,source={},target=/work", project.display());
        runner.args([
            "run",
            "--rm",
            "--init",
            "--memory",
            "2048m",
            "--memory-swap",
            "2048m",
            "--cpus",
            "2",
            "--pids-limit",
            "256",
            "--mount",
            &mount,
            "--workdir",
            CONTAINER_WORK_DIRECTORY,
            &image,
            "timeout",
            "900",
            "env",
            "CARGO_TARGET_DIR=/tmp/e1004-xtask",
            "CARGO_INCREMENTAL=0",
            &image_environment,
            "cargo",
            "run",
            "--locked",
            "-p",
            "xtask",
            "--",
            "verify-container",
            task,
        ]);
    }
    invalidate_task_results(task)?;
    let output = runner.output()?;
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    print!("{text}");
    fs::create_dir_all("verification/results")?;
    fs::write(format!("verification/results/{task}-container.log"), &text)?;
    if let Some(id) = text.lines().find_map(|line| {
        line.strip_prefix("Bounded container ")
            .and_then(|s| s.split_once(':'))
            .map(|(id, _)| id)
    }) {
        if id.len() == SHA256_HEX_BYTES && id.bytes().all(|b| b.is_ascii_hexdigit()) {
            let mounts = Command::new("docker")
                .args(["inspect", id, "--format", "{{json .Mounts}}"])
                .output()?;
            if !mounts.status.success() {
                return Err("cannot inspect verification result location".into());
            }
            let mounts: Vec<Mount> = serde_json::from_slice(&mounts.stdout)?;
            let root = std::env::current_dir()?.canonicalize()?;
            let shared = mounts.iter().any(|mount| {
                mount.kind == "bind"
                    && mount.source == root
                    && mount.destination == Path::new(CONTAINER_WORK_DIRECTORY)
            });
            let mut copied = true;
            if !shared {
                for artifact in xtask::evidence::result_artifacts(task)? {
                    let destination = Path::new("verification/results").join(artifact);
                    let mut source = format!("{id}:/work/verification/results/{artifact}");
                    if !artifact.ends_with(".json") {
                        fs::create_dir_all(&destination)?;
                        source.push_str("/.");
                    }
                    copied &= Command::new("docker")
                        .arg("cp")
                        .arg(source)
                        .arg(destination)
                        .status()?
                        .success();
                }
            }
            let removed = Command::new("docker").args(["rm", id]).status()?.success();
            if !copied || !removed {
                return Err("verification result transfer or container cleanup failed".into());
            }
        }
    }
    if matches!(
        output.status.code(),
        Some(TIMEOUT_EXIT | RESOURCE_KILL_EXIT)
    ) {
        return Err(format!("bounded {task} is inconclusive after a resource stop").into());
    }
    if !output.status.success() {
        return Err(format!("bounded {task} failed: {}", output.status).into());
    }
    Ok(())
}

pub fn build_image() -> VerifyResult {
    fs::create_dir_all("target")?;
    crate::run(
        "docker",
        &[
            "build",
            "--platform",
            "linux/amd64",
            "--iidfile",
            "target/verification-image.id",
            "--file",
            "verification/Dockerfile",
            ".",
        ],
    )?;
    crate::run(
        "docker",
        &[
            "build",
            "--iidfile",
            "target/host-verification-image.id",
            "--file",
            "verification/Host.Dockerfile",
            ".",
        ],
    )?;
    println!(
        "Verification image: {}",
        fs::read_to_string("target/verification-image.id")?.trim()
    );
    println!(
        "Host verification image: {}",
        fs::read_to_string("target/host-verification-image.id")?.trim()
    );
    Ok(())
}
