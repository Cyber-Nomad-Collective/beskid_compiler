use std::path::Path;
use std::process::Output;

pub fn assert_success(output: &Output, context: &str) {
    assert!(
        output.status.success(),
        "{context} failed.\nstatus: {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn assert_failure(output: &Output, context: &str) {
    assert!(
        !output.status.success(),
        "{context} unexpectedly succeeded.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn assert_output_contains(output: &Output, needle: &str, context: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains(needle) || stderr.contains(needle),
        "{context} output did not contain `{needle}`.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

pub fn assert_file_exists(path: &Path, context: &str) {
    assert!(path.is_file(), "{context}: missing file {}", path.display());
}

pub fn assert_exit_code(output: &Output, expected: i32, context: &str) {
    let actual = output.status.code();
    assert_eq!(
        actual,
        Some(expected),
        "{context}: unexpected exit code.\nexpected: {expected}\nactual: {:?}\nstdout:\n{}\nstderr:\n{}",
        actual,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
