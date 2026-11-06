#[path = "support/fuzz_receipt.rs"]
mod fuzz_receipt;

#[test]
fn completion_requires_success_and_a_complete_record_on_one_stream() {
    const COMPLETE: &str = "Done 10000 runs in 2 second(s)\n";
    const NOISE: &str = "INFO: seed corpus: files: 3\n";
    let cases = [
        (Some(0), Some(COMPLETE), Some(""), 10_000, true),
        (Some(0), Some(""), Some(COMPLETE), 10_000, true),
        (Some(0), Some(NOISE), Some(COMPLETE), 10_000, true),
        (Some(0), Some(COMPLETE), Some(NOISE), 10_000, true),
        (Some(1), Some(COMPLETE), Some(""), 10_000, false),
        (None, Some(COMPLETE), Some(""), 10_000, false),
        (Some(0), None, Some(COMPLETE), 10_000, false),
        (Some(0), Some(COMPLETE), None, 10_000, false),
        (Some(0), Some(""), Some(""), 10_000, false),
        (Some(0), Some(COMPLETE), Some(""), 10_001, false),
        (Some(0), Some("Done 0 runs"), Some(""), 0, false),
        (
            Some(0),
            Some("Done 10000 "),
            Some("runs in 2 second(s)"),
            10_000,
            false,
        ),
        (
            Some(0),
            Some("not Done 10000 runs"),
            Some(""),
            10_000,
            false,
        ),
        (
            Some(0),
            Some("Done 10000 runs-incomplete"),
            Some(""),
            10_000,
            false,
        ),
    ];
    for (exit, stdout, stderr, expected, accepted) in cases {
        assert_eq!(
            fuzz_receipt::completed(exit, stdout, stderr, expected),
            accepted,
            "exit={exit:?}, stdout={stdout:?}, stderr={stderr:?}, expected={expected}"
        );
    }
}
