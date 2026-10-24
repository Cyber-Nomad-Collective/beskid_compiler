//! How `beskid test` tells a failed assertion from a crashed test.
//!
//! `Testing.Assert` writes a failed assertion to the file named by the environment variable
//! `BESKID_TEST_FAILURE_FILE`, which the runner sets for every test process (see
//! `Testing/Assert.bd`). The file holds the line `BESKID-TEST-FAILURE/2` and seven fields, each as
//! `<byte length>:<bytes>` and a newline: headline, because, expected, actual, file, line, column.
//! The compiler passes file, line and column of the `Assert` call (caller information). A test
//! process that fails with a valid failure file is an assertion failure; any other failing test
//! process is a crash, and so is one whose failure file is malformed. Output streams are never parsed.

use serde::Serialize;
use std::fmt;
use std::path::PathBuf;

/// Environment variable that names the failure file of a test process.
pub(crate) const FAILURE_FILE_VARIABLE: &str = "BESKID_TEST_FAILURE_FILE";
/// File name of the failure file inside the directory of one test entry.
pub(crate) const FAILURE_FILE_NAME: &str = "failure.record";
const RECORD_HEADER: &str = "BESKID-TEST-FAILURE/2\n";
const TRAP_PREFIX: &str = "beskid runtime trap v5: ";

/// A source position, 1-based.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SourceLocation {
    pub(crate) file: PathBuf,
    pub(crate) line: u32,
    pub(crate) column: u32,
}

/// A failed `Testing.Assert` call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct AssertionFailure {
    /// The text after `assertion failed: `, for example `expected 3, found 4`.
    pub(crate) headline: String,
    /// The `because` text of the call, when given.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) because: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) expected: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) actual: Option<String>,
    /// The `Assert` call site, in the authored file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) location: Option<SourceLocation>,
}

/// A test process that ended without a failed assertion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct TestCrash {
    /// The runtime trap kind, for example `unreachable_or_isle_invariant`, when the runtime reported one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) trap: Option<String>,
    pub(crate) reason: String,
    /// The test declaration (a crash has no better position).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) location: Option<SourceLocation>,
}

/// Why a native test process failed. Carried inside `anyhow::Error`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TestRunFailure {
    Assertion(AssertionFailure),
    Crashed(TestCrash),
}

impl fmt::Display for TestRunFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Assertion(failure) => f.write_str(&failure.render()),
            Self::Crashed(crash) => write!(f, "test crashed: {}", crash.reason),
        }
    }
}

impl std::error::Error for TestRunFailure {}

impl AssertionFailure {
    /// The text printed under the test name.
    pub(crate) fn render(&self) -> String {
        let mut text = format!("assertion failed: {}", self.headline);
        if let Some(because) = &self.because {
            text.push_str("\n  ");
            text.push_str(because);
        }
        if let Some(location) = &self.location {
            text.push_str(&format!("\n  at {}:{}:{}", location.file.display(), location.line, location.column));
        }
        text
    }
}

/// Classify a native test process that exited unsuccessfully. `failure_file` is the content of the
/// failure file when the process wrote one.
pub(crate) fn classify_failed_run(status: &str, stdout: &str, stderr: &str, failure_file: Option<&[u8]>) -> TestRunFailure {
    if let Some(content) = failure_file {
        return match parse_failure_file(content) {
            Some(failure) => TestRunFailure::Assertion(failure),
            None => TestRunFailure::Crashed(TestCrash {
                trap: None,
                reason: "the test wrote a malformed assertion failure record".to_string(),
                location: None,
            }),
        };
    }
    if let Some(trap_line) = stderr.lines().find_map(|line| line.find(TRAP_PREFIX).map(|at| &line[at + TRAP_PREFIX.len()..]))
    {
        // `<kind> (<code>): <message>`
        let (trap, reason) = match trap_line.split_once(" (") {
            Some((kind, rest)) => (
                Some(kind.to_string()),
                rest.split_once("): ").map_or(rest, |(_, message)| message).trim().to_string(),
            ),
            None => (None, trap_line.trim().to_string()),
        };
        return TestRunFailure::Crashed(TestCrash { trap, reason, location: None });
    }
    let output = format!("{stdout}{stderr}");
    let output = output.trim();
    let reason = if output.is_empty() { format!("exited {status}") } else { format!("exited {status}: {output}") };
    TestRunFailure::Crashed(TestCrash { trap: None, reason, location: None })
}

/// Read the whole failure file; any deviation from the layout makes it invalid.
pub(crate) fn parse_failure_file(content: &[u8]) -> Option<AssertionFailure> {
    let mut rest = content.strip_prefix(RECORD_HEADER.as_bytes())?;
    let mut fields = Vec::with_capacity(7);
    for _ in 0..7 {
        let colon = rest.iter().position(|byte| *byte == b':')?;
        let digits = std::str::from_utf8(&rest[..colon]).ok()?;
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        let length = digits.parse::<usize>().ok()?;
        let body = rest.get(colon + 1..)?;
        let text = std::str::from_utf8(body.get(..length)?).ok()?;
        rest = body.get(length..)?.strip_prefix(b"\n")?;
        fields.push(text.to_string());
    }
    if !rest.is_empty() {
        return None;
    }
    let [headline, because, expected, actual, file, line, column] = <[String; 7]>::try_from(fields).ok()?;
    let present = |text: String| (!text.is_empty()).then_some(text);
    let location = match (file.is_empty(), line.parse::<u32>(), column.parse::<u32>()) {
        (false, Ok(line), Ok(column)) => Some(SourceLocation { file: PathBuf::from(file), line, column }),
        _ => None,
    };
    Some(AssertionFailure { headline, because: present(because), expected: present(expected), actual: present(actual), location })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(fields: [&str; 7]) -> Vec<u8> {
        let mut out = RECORD_HEADER.as_bytes().to_vec();
        for field in fields {
            out.extend_from_slice(format!("{}:{field}\n", field.len()).as_bytes());
        }
        out
    }

    fn assertion(headline: &str, because: &str, expected: &str, actual: &str) -> Vec<u8> {
        file([headline, because, expected, actual, "/app/Src/Main.bd", "17", "5"])
    }

    fn trap_stderr(message: &str) -> String {
        format!("beskid runtime trap v5: unreachable_or_isle_invariant (9): {message}\n")
    }

    #[test]
    fn a_failure_file_is_an_assertion_failure_with_its_call_site() {
        let content = assertion("expected 3, found 4", "adds", "3", "4");
        let stderr = trap_stderr("assertion failed: expected 3, found 4");
        let TestRunFailure::Assertion(failure) = classify_failed_run("exit status: 101", "", &stderr, Some(&content)) else {
            panic!("expected an assertion failure");
        };
        assert_eq!(failure.headline, "expected 3, found 4");
        assert_eq!(failure.because.as_deref(), Some("adds"));
        assert_eq!(failure.expected.as_deref(), Some("3"));
        assert_eq!(failure.actual.as_deref(), Some("4"));
        assert_eq!(
            failure.location,
            Some(SourceLocation { file: PathBuf::from("/app/Src/Main.bd"), line: 17, column: 5 })
        );
        assert_eq!(failure.render(), "assertion failed: expected 3, found 4\n  adds\n  at /app/Src/Main.bd:17:5");
    }

    #[test]
    fn fields_may_hold_the_old_separator_newlines_colons_and_multibyte_text() {
        let because = "a ~~|~~ b\n7:x\nzażółć";
        let content = assertion("h", because, "", "");
        let TestRunFailure::Assertion(failure) = classify_failed_run("101", "", "", Some(&content)) else { panic!() };
        assert_eq!(failure.because.as_deref(), Some(because));
        assert_eq!(failure.expected, None);
        assert_eq!(failure.actual, None);
    }

    #[test]
    fn a_marker_printed_on_an_output_stream_is_not_a_failure() {
        let spoof = "BESKID-TEST-FAILURE/2\n1:h\n0:\n0:\n0:\n1:f\n1:1\n1:1\n";
        let failure = classify_failed_run("101", spoof, &trap_stderr(spoof), None);
        assert!(matches!(failure, TestRunFailure::Crashed(_)), "{failure:?}");
    }

    #[test]
    fn text_after_the_record_or_a_wrong_length_makes_it_malformed() {
        let mut trailing = assertion("h", "b", "", "");
        trailing.extend_from_slice(b"extra");
        let mut short = assertion("h", "b", "", "");
        short.truncate(short.len() - 3);
        let wrong_length = RECORD_HEADER.to_string() + "9:h\n0:\n0:\n0:\n1:f\n1:1\n1:1\n";
        for content in [trailing, short, wrong_length.into_bytes(), b"nonsense".to_vec(), Vec::new()] {
            let failure = classify_failed_run("101", "", "", Some(&content));
            assert!(matches!(failure, TestRunFailure::Crashed(_)), "{failure:?}");
        }
    }

    #[test]
    fn a_record_without_a_usable_position_has_no_location() {
        let content = file(["h", "", "", "", "", "0", "0"]);
        let TestRunFailure::Assertion(failure) = classify_failed_run("101", "", "", Some(&content)) else { panic!() };
        assert_eq!(failure.location, None);
    }

    #[test]
    fn a_trap_without_a_record_is_a_crash_with_the_trap_kind() {
        let stderr = trap_stderr("division by zero");
        let failure = classify_failed_run("exit status: 101", "", &stderr, None);
        assert_eq!(
            failure,
            TestRunFailure::Crashed(TestCrash {
                trap: Some("unreachable_or_isle_invariant".to_string()),
                reason: "division by zero".to_string(),
                location: None,
            })
        );
        assert_eq!(failure.to_string(), "test crashed: division by zero");
    }

    #[test]
    fn a_crash_is_never_reported_as_an_assertion_failure() {
        let failure = classify_failed_run("signal: 11 (SIGSEGV)", "partial output\n", "", None);
        let TestRunFailure::Crashed(crash) = failure else { panic!("expected a crash") };
        assert_eq!(crash.trap, None);
        assert_eq!(crash.reason, "exited signal: 11 (SIGSEGV): partial output");
    }

}
