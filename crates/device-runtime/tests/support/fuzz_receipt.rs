pub fn completed(
    exit: Option<i64>,
    stdout: Option<&str>,
    stderr: Option<&str>,
    expected: u64,
) -> bool {
    let (Some(0), Some(stdout), Some(stderr)) = (exit, stdout, stderr) else {
        return false;
    };
    if expected == 0 {
        return false;
    }
    let marker = format!("Done {expected} runs");
    let complete = |text: &str| {
        text.lines().any(|line| {
            line == marker.as_str()
                || line
                    .strip_prefix(marker.as_str())
                    .is_some_and(|suffix| suffix.starts_with(" in "))
        })
    };
    complete(stdout) || complete(stderr)
}
