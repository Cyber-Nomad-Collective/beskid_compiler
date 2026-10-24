//! How `beskid test` tells a failed assertion from a crashed test, and where the assertion sits.
//!
//! `Testing.Assert` ends a failing test with a framed record (see `Testing/Assert.bd`):
//! `BESKID-TEST-FAILURE/1`, then headline, because text, expected text, actual text, file, line and
//! column, each preceded by the separator `~~|~~`. The compiler passes file, line and column of the
//! `Assert` call (caller information). The runtime prints the record inside its trap line. A test
//! process that fails with a record is an assertion failure; any other failing test process is a
//! crash.

use serde::Serialize;
use std::fmt;
use std::path::PathBuf;

const RECORD_MARKER: &str = "BESKID-TEST-FAILURE/1";
const FIELD_SEPARATOR: &str = "~~|~~";
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

/// Classify the output of a native test process that exited unsuccessfully.
pub(crate) fn classify_failed_run(status: &str, stdout: &str, stderr: &str) -> TestRunFailure {
    if let Some(failure) = parse_assertion_record(stderr).or_else(|| parse_assertion_record(stdout)) {
        return TestRunFailure::Assertion(failure);
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

/// Read the failure record from process output, when there is one.
pub(crate) fn parse_assertion_record(output: &str) -> Option<AssertionFailure> {
    let start = output.find(RECORD_MARKER)?;
    let record = &output[start + RECORD_MARKER.len()..];
    let record = record.strip_prefix(FIELD_SEPARATOR)?;
    // The record runs to the end of the output; the runtime appends one newline.
    let record = record.strip_suffix('\n').unwrap_or(record);
    let fields = record.split(FIELD_SEPARATOR).collect::<Vec<_>>();
    let [headline, because, expected, actual, file, line, column] = fields.as_slice() else { return None };
    let present = |text: &str| (!text.is_empty()).then(|| text.to_string());
    let location = match (file.is_empty(), line.parse::<u32>(), column.parse::<u32>()) {
        (false, Ok(line), Ok(column)) => Some(SourceLocation { file: PathBuf::from(file), line, column }),
        _ => None,
    };
    Some(AssertionFailure {
        headline: (*headline).to_string(),
        because: present(because),
        expected: present(expected),
        actual: present(actual),
        location,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(headline: &str, because: &str, expected: &str, actual: &str) -> String {
        format!(
            "{RECORD_MARKER}~~|~~{headline}~~|~~{because}~~|~~{expected}~~|~~{actual}~~|~~/app/Src/Main.bd~~|~~17~~|~~5"
        )
    }

    fn trap_stderr(message: &str) -> String {
        format!("beskid runtime trap v5: unreachable_or_isle_invariant (9): {message}\n")
    }

    #[test]
    fn an_assertion_record_inside_a_trap_line_is_an_assertion_failure_with_its_call_site() {
        let stderr = trap_stderr(&record("expected 3, found 4", "adds", "3", "4"));
        let TestRunFailure::Assertion(failure) = classify_failed_run("exit status: 101", "", &stderr) else {
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
    fn empty_fields_are_absent_and_multiline_because_survives() {
        let stderr = trap_stderr(&record("the values are not equal", "line one\nline two", "", ""));
        let TestRunFailure::Assertion(failure) = classify_failed_run("101", "", &stderr) else { panic!() };
        assert_eq!(failure.because.as_deref(), Some("line one\nline two"));
        assert_eq!(failure.expected, None);
        assert_eq!(failure.actual, None);
    }

    #[test]
    fn a_record_without_a_usable_position_has_no_location() {
        let stderr = trap_stderr(&format!("{RECORD_MARKER}~~|~~h~~|~~~~|~~~~|~~~~|~~~~|~~0~~|~~0"));
        let TestRunFailure::Assertion(failure) = classify_failed_run("101", "", &stderr) else { panic!() };
        assert_eq!(failure.location, None);
    }

    #[test]
    fn a_trap_without_a_record_is_a_crash_with_the_trap_kind() {
        let stderr = trap_stderr("division by zero");
        let failure = classify_failed_run("exit status: 101", "", &stderr);
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
        let failure = classify_failed_run("signal: 11 (SIGSEGV)", "partial output\n", "");
        let TestRunFailure::Crashed(crash) = failure else { panic!("expected a crash") };
        assert_eq!(crash.trap, None);
        assert_eq!(crash.reason, "exited signal: 11 (SIGSEGV): partial output");
    }

    #[test]
    fn a_malformed_record_is_a_crash() {
        let stderr = trap_stderr("BESKID-TEST-FAILURE/1~~|~~only a headline");
        assert!(matches!(classify_failed_run("101", "", &stderr), TestRunFailure::Crashed(_)));
    }
}
