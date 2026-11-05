include!("../src/verification.rs");

fn run(program: &str, arguments: &[&str]) -> VerifyResult {
    let status = Command::new(program).args(arguments).status()?;
    if !status.success() {
        return Err(format!("{program} exited with {status}").into());
    }
    Ok(())
}

#[test]
fn verification_progress_keeps_the_exact_report_bytes_and_propagates_output_failure() {
    let bytes = (0..PROGRESS_CHUNK_BYTES * 2 + 1)
        .map(|index| index as u8)
        .collect::<Vec<_>>();
    let mut mirrored = Vec::new();
    assert_eq!(
        mirror_output(bytes.as_slice(), &mut mirrored).unwrap(),
        bytes
    );
    assert_eq!(mirrored, bytes);
    assert!(mirror_output(&[][..], &mut mirrored).unwrap().is_empty());
    assert!(mirror_output(&b"progress"[..], &mut &mut [][..]).is_err());
}

struct WorkingDirectory {
    previous: Option<PathBuf>,
    fixture: PathBuf,
}
impl WorkingDirectory {
    fn fixture() -> Self {
        let previous = std::env::current_dir().unwrap();
        let fixture =
            std::env::temp_dir().join(format!("e1004-receipt-integrity-{}", std::process::id()));
        fs::create_dir(&fixture).unwrap();
        let directory = Self {
            previous: Some(previous),
            fixture,
        };
        std::env::set_current_dir(&directory.fixture).unwrap();
        directory
    }
}
impl Drop for WorkingDirectory {
    fn drop(&mut self) {
        if let Some(previous) = &self.previous {
            std::env::set_current_dir(previous).unwrap();
        }
        fs::remove_dir_all(&self.fixture).unwrap();
        let snapshot = self.fixture.with_extension("snapshot");
        if snapshot.exists() {
            fs::remove_dir_all(snapshot).unwrap();
        }
    }
}

#[test]
fn production_receipt_validation_rejects_artifact_subject_source_format_and_exit_changes() {
    let _directory = WorkingDirectory::fixture();
    for path in xtask::evidence::verification_inputs(&[]) {
        fs::create_dir_all(Path::new(path).parent().unwrap()).unwrap();
        fs::write(path, b"fixed producer fixture").unwrap();
    }
    let source = "crates/device-core/src/fixture.rs";
    fs::create_dir_all(Path::new(source).parent().unwrap()).unwrap();
    fs::write(source, b"fixed product fixture").unwrap();
    for name in ["core-mutations-0", "core-mutations-1"] {
        fs::create_dir_all(format!("verification/results/{name}")).unwrap();
        fs::write(format!("verification/results/{name}/outcomes.json"), b"{}").unwrap();
    }
    let name = "core-mutations-0";
    fs::create_dir_all("target").unwrap();
    let image = format!("sha256:{}", "a".repeat(SHA256_HEX_BYTES));
    fs::write("target/host-verification-image.id", &image).unwrap();
    let path = format!("verification/results/{name}.json");
    let original = Receipt {
        version: RECEIPT_FORMAT,
        command: vec!["receipt authority fixture".into()],
        source_sha256: identity(name, xtask::evidence::receipt_scope(name).unwrap()).unwrap(),
        artifact_sha256: Some(format!("{:x}", Sha256::digest(b"{}"))),
        container_image: Some(image),
        exit: Some(0),
        stdout: "fixture report".into(),
        stderr: String::new(),
    };
    let bytes = serde_json::to_vec(&original).unwrap();
    let inputs = input_hashes(
        Path::new("."),
        xtask::evidence::receipt_scope(name).unwrap(),
    )
    .unwrap();
    fs::write(&path, &bytes).unwrap();
    assert_eq!(current(name, true).unwrap(), "fixture report\n");

    let artifact = format!("verification/results/{name}/outcomes.json");
    fs::write(&artifact, b"{ }").unwrap();
    assert!(current(name, true)
        .unwrap_err()
        .to_string()
        .contains("mutation artifact does not match its receipt"));
    fs::write(&artifact, b"{}").unwrap();
    assert!(current(name, true).is_ok());

    fs::write("verification/results/core-mutations-1.json", &bytes).unwrap();
    assert!(current("core-mutations-1", true)
        .unwrap_err()
        .to_string()
        .contains("stale verification result: core-mutations-1"));

    for path in [source, "xtask/src/verification.rs"] {
        let original_bytes = fs::read(path).unwrap();
        fs::write(path, b"changed fixture").unwrap();
        assert!(current(name, true)
            .unwrap_err()
            .to_string()
            .contains("stale verification result: core-mutations-0"));
        fs::write(path, original_bytes).unwrap();
        assert!(current(name, true).is_ok());
    }

    assert!(current(name, false)
        .unwrap_err()
        .to_string()
        .contains("unsuccessful verification result"));
    for exit in [None, Some(1)] {
        let mut invalid: Receipt = serde_json::from_slice(&bytes).unwrap();
        invalid.exit = exit;
        fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
        assert!(current(name, true)
            .unwrap_err()
            .to_string()
            .contains("unsuccessful verification result"));
    }
    let mut invalid: Receipt = serde_json::from_slice(&bytes).unwrap();
    invalid.version = 0;
    fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
    assert!(current(name, true)
        .unwrap_err()
        .to_string()
        .contains("stale verification result"));
    fs::write(&path, &bytes).unwrap();
    assert!(current(name, true).is_ok());
    let other = "verification/results/core-mutations-1.json";
    let mut wrong_image: Receipt = serde_json::from_slice(&bytes).unwrap();
    wrong_image.container_image = Some(format!("sha256:{}", "b".repeat(SHA256_HEX_BYTES)));
    fs::write(&path, serde_json::to_vec(&wrong_image).unwrap()).unwrap();
    assert!(
        current(name, true).is_err(),
        "an image substitution must invalidate the receipt"
    );
    fs::write(&path, &bytes).unwrap();
    fs::write(other, &bytes).unwrap();
    invalidate_task_results("mutations-core-shard-0").unwrap();
    assert!(
        !Path::new(&path).exists(),
        "a pending retry must not inherit its previous success"
    );
    assert_eq!(fs::read(other).unwrap(), bytes);
    invalidate_task_results("mutations-core-shard-0").unwrap();
    fs::write(source, b"changed during capture").unwrap();
    let rejected = publish_receipt(name, &inputs, &original).unwrap_err();
    assert!(rejected.to_string().contains(source));
    fs::write(source, b"fixed product fixture").unwrap();
    assert!(
        current(name, true).is_err(),
        "restoring source must not revive a receipt from a rejected attempt"
    );
    fs::create_dir("framing-inputs").unwrap();
    fs::write("framing-inputs/a", b"").unwrap();
    fs::write("framing-inputs/b", b"payload").unwrap();
    let separate = identity("framing", &["framing-inputs"]).unwrap();
    fs::write("framing-inputs/a", b"framing-inputs/b\0payload").unwrap();
    fs::remove_file("framing-inputs/b").unwrap();
    assert_ne!(
        identity("framing", &["framing-inputs"]).unwrap(), separate,
        "file contents must not impersonate another input's name and bytes"
    );
}

#[test]
fn verification_copies_isolate_source_and_exclude_existing_results_and_build_outputs() {
    let fixture = std::env::temp_dir().join(format!("e1004-snapshot-{}", std::process::id()));
    fs::create_dir(&fixture).unwrap();
    let directory = WorkingDirectory {
        previous: None,
        fixture,
    };
    for path in [
        "src/lib.rs",
        "target/build",
        "firmware/target/build",
        ".git/index",
        "verification/results/old.json",
        "verification/inventory.toml",
    ] {
        fs::create_dir_all(directory.fixture.join(path).parent().unwrap()).unwrap();
        fs::write(directory.fixture.join(path), b"captured input").unwrap();
    }
    for path in
        xtask::evidence::verification_inputs(xtask::evidence::receipt_scope("tool-proofs").unwrap())
    {
        let path = directory.fixture.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"captured input").unwrap();
    }
    let destination = directory.fixture.with_extension("snapshot");
    capture_workspace(&directory.fixture, &destination, "proofs-tools").unwrap();
    assert!(destination.join("src/lib.rs").is_file());
    assert!(destination.join("verification/inventory.toml").is_file());
    for path in ["target", "firmware/target", ".git", "verification/results"] {
        assert!(
            !destination.join(path).exists(),
            "generated or private input copied: {path}"
        );
    }
    fs::write(directory.fixture.join("src/lib.rs"), b"later edit").unwrap();
    assert_eq!(
        fs::read(destination.join("src/lib.rs")).unwrap(),
        b"captured input"
    );
    assert!(copy_workspace(&directory.fixture, &destination).is_err());
    #[cfg(unix)]
    {
        fs::remove_dir_all(&destination).unwrap();
        std::os::unix::fs::symlink(
            directory.fixture.join("src/lib.rs"),
            directory.fixture.join("source-link"),
        )
        .unwrap();
        assert!(copy_workspace(&directory.fixture, &destination)
            .unwrap_err()
            .to_string()
            .contains("source-link"));
    }
}
