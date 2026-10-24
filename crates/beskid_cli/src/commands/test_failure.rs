//! How `beskid test` tells a failed assertion from a crashed test, and where the assertion sits.
//!
//! `Testing.Assert` ends a failing test with a framed record (see `Testing/Assert.bd`):
//! `BESKID-TEST-FAILURE/1`, then headline, because text, expected text and actual text, each
//! preceded by the separator `~~|~~`. The runtime prints the record inside its trap line. A test
//! process that fails with a record is an assertion failure; any other failing test process is a
//! crash. The record has no call site, so the runner finds the `Assert` call in the project
//! sources from the because text.

use serde::Serialize;
use std::fmt;
use std::path::{Path, PathBuf};

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
    /// The `Assert` call site, or the test declaration when the call could not be found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) location: Option<SourceLocation>,
    /// True when `location` is the `Assert` call, false when it is the test declaration.
    pub(crate) location_is_call_site: bool,
}

/// A test process that ended without a failed assertion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct TestCrash {
    /// The runtime trap kind, for example `unreachable_or_isle_invariant`, when the runtime reported one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) trap: Option<String>,
    pub(crate) reason: String,
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
        return TestRunFailure::Crashed(TestCrash { trap, reason });
    }
    let output = format!("{stdout}{stderr}");
    let output = output.trim();
    let reason = if output.is_empty() { format!("exited {status}") } else { format!("exited {status}: {output}") };
    TestRunFailure::Crashed(TestCrash { trap: None, reason })
}

/// Read the failure record from process output, when there is one.
pub(crate) fn parse_assertion_record(output: &str) -> Option<AssertionFailure> {
    let start = output.find(RECORD_MARKER)?;
    let record = &output[start + RECORD_MARKER.len()..];
    let record = record.strip_prefix(FIELD_SEPARATOR)?;
    // The record runs to the end of the output; the runtime appends one newline.
    let record = record.strip_suffix('\n').unwrap_or(record);
    let fields = record.split(FIELD_SEPARATOR).collect::<Vec<_>>();
    let [headline, because, expected, actual] = fields.as_slice() else { return None };
    let present = |text: &str| (!text.is_empty()).then(|| text.to_string());
    Some(AssertionFailure {
        headline: (*headline).to_string(),
        because: present(because),
        expected: present(expected),
        actual: present(actual),
        location: None,
        location_is_call_site: false,
    })
}

/// A `.bd` file of the tested project.
pub(crate) struct SourceFile {
    pub(crate) path: PathBuf,
    pub(crate) text: String,
}

/// Read the `.bd` files below `root`, skipping build output and hidden directories.
pub(crate) fn read_project_sources(root: &Path) -> Vec<SourceFile> {
    const LIMIT: usize = 4000;
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else { continue };
        let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() {
                if !name.starts_with('.') && name != "obj" && name != "target" {
                    pending.push(path);
                }
            } else if kind.is_file() && name.ends_with(".bd") && files.len() < LIMIT {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    files.push(SourceFile { path, text });
                }
            }
        }
    }
    files
}

/// Find the `Assert`/`Expect` call that raised `failure`. The column is the start of the `Assert` or `Expect` word.
///
/// A call matches when its arguments contain the because text as a string literal. With several
/// matches the first one at or after `test_line` in `test_file` wins, then the first overall.
/// Returns `None` when no call matches.
pub(crate) fn locate_assertion(
    sources: &[SourceFile],
    failure: &AssertionFailure,
    test_file: &Path,
    test_line: u32,
) -> Option<SourceLocation> {
    let because = failure.because.as_deref()?;
    let literal = format!("\"{because}\"");
    let mut matches = Vec::new();
    for source in sources {
        for (line_index, line) in source.text.lines().enumerate() {
            let mut from = 0;
            while let Some(at) = find_call(line, from) {
                from = at + 1;
                let arguments = call_arguments(&source.text, line_index, at);
                if arguments.contains(&literal) {
                    matches.push(SourceLocation {
                        file: source.path.clone(),
                        line: line_index as u32 + 1,
                        column: line[..at].chars().count() as u32 + 1,
                    });
                }
            }
        }
    }
    matches
        .iter()
        .find(|location| location.file == test_file && location.line >= test_line)
        .or_else(|| matches.first())
        .cloned()
}

/// Byte offset of the next `Assert.`/`Expect.` call at or after `from`.
fn find_call(line: &str, from: usize) -> Option<usize> {
    ["Assert.", "Expect."]
        .iter()
        .filter_map(|prefix| {
            let mut search = from;
            while let Some(found) = line[search..].find(prefix) {
                let at = search + found;
                let before = line[..at].chars().next_back();
                if before.is_none_or(|c| !(c.is_alphanumeric() || c == '_')) {
                    return Some(at);
                }
                search = at + prefix.len();
            }
            None
        })
        .min()
}

/// The text of the call's argument list, from the opening parenthesis to its match; strings are
/// skipped as units and the list may span lines.
fn call_arguments(text: &str, line_index: usize, column_byte: usize) -> String {
    let mut lines = text.lines().skip(line_index);
    let mut result = String::new();
    let mut depth = 0usize;
    let mut started = false;
    let mut in_string = false;
    let mut escaped = false;
    let mut first = true;
    let mut read = 0;
    while let Some(line) = lines.next() {
        read += 1;
        if !started && read > 3 {
            break;
        }
        let line = if first { &line[column_byte..] } else { line };
        first = false;
        for c in line.chars() {
            if started {
                result.push(c);
            }
            if in_string {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    in_string = false;
                }
                continue;
            }
            match c {
                '"' => in_string = true,
                '(' => {
                    depth += 1;
                    if !started {
                        started = true;
                        result.push(c);
                    }
                }
                ')' if started => {
                    depth -= 1;
                    if depth == 0 {
                        return result;
                    }
                }
                _ => {}
            }
        }
        if started {
            result.push('\n');
        }
        if result.len() > 4096 {
            break;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(headline: &str, because: &str, expected: &str, actual: &str) -> String {
        format!("{RECORD_MARKER}~~|~~{headline}~~|~~{because}~~|~~{expected}~~|~~{actual}")
    }

    fn trap_stderr(message: &str) -> String {
        format!("beskid runtime trap v5: unreachable_or_isle_invariant (9): {message}\n")
    }

    #[test]
    fn an_assertion_record_inside_a_trap_line_is_an_assertion_failure() {
        let stderr = trap_stderr(&record("expected 3, found 4", "adds", "3", "4"));
        let TestRunFailure::Assertion(failure) =
            classify_failed_run("exit status: 101", "", &stderr)
        else {
            panic!("expected an assertion failure");
        };
        assert_eq!(failure.headline, "expected 3, found 4");
        assert_eq!(failure.because.as_deref(), Some("adds"));
        assert_eq!(failure.expected.as_deref(), Some("3"));
        assert_eq!(failure.actual.as_deref(), Some("4"));
        assert_eq!(failure.render(), "assertion failed: expected 3, found 4\n  adds");
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
    fn a_trap_without_a_record_is_a_crash_with_the_trap_kind() {
        let stderr = trap_stderr("division by zero");
        let failure = classify_failed_run("exit status: 101", "", &stderr);
        assert_eq!(
            failure,
            TestRunFailure::Crashed(TestCrash {
                trap: Some("unreachable_or_isle_invariant".to_string()),
                reason: "division by zero".to_string()
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

    fn failure(because: &str) -> AssertionFailure {
        parse_assertion_record(&record("expected 3, found 4", because, "3", "4")).unwrap()
    }

    fn sources(text: &str) -> Vec<SourceFile> {
        vec![SourceFile { path: PathBuf::from("Src/Main.bd"), text: text.to_string() }]
    }

    #[test]
    fn the_call_site_is_found_from_the_because_text() {
        let text = "test AddsNumbers {\n    Assert.Equal(1, 1, \"first\");\n    Assert.Equal(Add(1, 3), 3, \"adds\");\n}\n";
        let location = locate_assertion(&sources(text), &failure("adds"), Path::new("Src/Main.bd"), 1).unwrap();
        assert_eq!((location.line, location.column), (3, 5));
    }

    #[test]
    fn a_qualified_call_reports_the_start_of_the_qualified_name() {
        let text = "test T {\n    Testing.Assert.True(ok, \"must hold\");\n}\n";
        let location = locate_assertion(&sources(text), &failure("must hold"), Path::new("Src/Main.bd"), 1).unwrap();
        assert_eq!((location.line, location.column), (2, 13));
    }

    #[test]
    fn a_call_spanning_lines_is_found() {
        let text = "test T {\n    Assert.Equal(\n        Add(1, 3),\n        3,\n        \"adds\");\n}\n";
        let location = locate_assertion(&sources(text), &failure("adds"), Path::new("Src/Main.bd"), 1).unwrap();
        assert_eq!(location.line, 2);
    }

    #[test]
    fn several_matches_prefer_the_one_in_the_failing_test() {
        let text = "test Early {\n    Assert.True(a, \"same\");\n}\ntest Late {\n    Assert.True(b, \"same\");\n}\n";
        let location = locate_assertion(&sources(text), &failure("same"), Path::new("Src/Main.bd"), 4).unwrap();
        assert_eq!(location.line, 5);
    }

    #[test]
    fn a_because_text_with_no_matching_call_has_no_location() {
        let text = "test T {\n    Assert.True(a, \"other\");\n}\n";
        assert_eq!(locate_assertion(&sources(text), &failure("missing"), Path::new("Src/Main.bd"), 1), None);
        assert_eq!(locate_assertion(&sources(text), &parse_assertion_record(&record("h", "", "", "")).unwrap(), Path::new("Src/Main.bd"), 1), None);
    }

    #[test]
    fn identifiers_that_merely_end_in_assert_are_not_calls() {
        let text = "test T {\n    MyAssert.True(a, \"adds\");\n}\n";
        assert_eq!(locate_assertion(&sources(text), &failure("adds"), Path::new("Src/Main.bd"), 1), None);
    }
}
