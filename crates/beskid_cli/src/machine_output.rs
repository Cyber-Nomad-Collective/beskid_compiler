//! Versioned newline-delimited JSON records for CLI-owned results (`--json`).
//!
//! Every record is one JSON object on one stdout line with `schemaVersion`, `command`, `event` and
//! `status`. Each command emits zero or more detail records followed by exactly one `result` record
//! whose status is `success` or `failure`. Progress and human diagnostics stay on stderr, so stdout
//! carries only these records. [`OUTPUT_SCHEMAS`] is the single inventory that `beskid dev
//! capabilities --json` publishes.
#![allow(non_snake_case)]
use anyhow::Result;
use beskid_analysis::analysis::{SemanticDiagnostic, Severity};
use serde_json::{Value, json};
use std::io::Write;

/// Version of every record schema in [`OUTPUT_SCHEMAS`].
pub const SCHEMA_VERSION: u64 = 1;

/// One command's record vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputSchema {
    pub command: &'static str,
    /// Event names in emission order; `result` is always last and emitted exactly once.
    pub events: &'static [&'static str],
}

/// Commands with `--json` output and the events each one emits.
pub const OUTPUT_SCHEMAS: [OutputSchema; 7] = [
    OutputSchema { command: "check", events: &["diagnostic", "result"] },
    OutputSchema { command: "build", events: &["diagnostic", "result"] },
    OutputSchema { command: "test", events: &["result"] },
    OutputSchema { command: "add", events: &["change", "result"] },
    OutputSchema { command: "remove", events: &["change", "result"] },
    OutputSchema { command: "update", events: &["change", "result"] },
    OutputSchema { command: "doctor", events: &["finding", "result"] },
];

/// Capability-document entries for [`OUTPUT_SCHEMAS`].
pub fn SchemaDescriptions() -> Vec<Value> {
    OUTPUT_SCHEMAS
        .iter()
        .map(|schema| {
            json!({
                "command": schema.command,
                "schemaVersion": SCHEMA_VERSION,
                "format": "ndjson",
                "events": schema.events,
            })
        })
        .collect()
}

/// Names of the commands that support `--json`, for usage guidance.
pub fn JsonCommandList() -> String {
    OUTPUT_SCHEMAS.iter().map(|schema| schema.command).collect::<Vec<_>>().join(", ")
}

/// Build one record. `fields` must be a JSON object and must not reuse a common key.
pub fn Record(command: &str, event: &str, status: &str, fields: Value) -> Value {
    let mut record = serde_json::Map::new();
    record.insert("schemaVersion".into(), json!(SCHEMA_VERSION));
    record.insert("command".into(), json!(command));
    record.insert("event".into(), json!(event));
    record.insert("status".into(), json!(status));
    match fields {
        Value::Object(fields) => {
            for (key, value) in fields {
                debug_assert!(!record.contains_key(&key), "record field `{key}` shadows a common key");
                record.entry(key).or_insert(value);
            }
        }
        Value::Null => {}
        other => panic!("record fields must be a JSON object, got {other}"),
    }
    Value::Object(record)
}

/// Stable severity token of a diagnostic record status.
pub fn SeverityName(severity: &Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Note => "note",
    }
}

/// Diagnostic fields: code, message, help and a span with byte offsets plus 1-based line and
/// column, where columns count Unicode scalar values.
pub fn DiagnosticFields(diagnostic: &SemanticDiagnostic) -> Value {
    let source = diagnostic.src.inner();
    let offset = diagnostic.span.offset();
    let length = diagnostic.span.len();
    let span = (offset <= source.len()).then(|| {
        let end = (offset + length).min(source.len());
        json!({
            "file": diagnostic.src.name(),
            "offset": offset,
            "length": end - offset,
            "start": LineColumn(source, offset),
            "end": LineColumn(source, end),
        })
    });
    json!({
        "diagnostic": {
            "code": diagnostic.code,
            "severity": SeverityName(&diagnostic.severity),
            "message": diagnostic.message,
            "label": diagnostic.label,
            "help": diagnostic.help,
            "span": span,
        }
    })
}

fn LineColumn(source: &str, offset: usize) -> Value {
    let mut boundary = offset.min(source.len());
    while !source.is_char_boundary(boundary) {
        boundary -= 1;
    }
    let before = &source[..boundary];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    let column = before[line_start..].chars().count() + 1;
    json!({ "line": line, "column": column })
}

/// Record writer for one command invocation.
pub struct Records {
    command: &'static str,
}

impl Records {
    pub fn new(command: &'static str) -> Self {
        debug_assert!(OUTPUT_SCHEMAS.iter().any(|schema| schema.command == command), "{command} has no schema");
        Self { command }
    }

    /// Write one record line to stdout and flush it.
    pub fn Emit(&self, event: &'static str, status: &str, fields: Value) -> Result<()> {
        debug_assert!(
            OUTPUT_SCHEMAS
                .iter()
                .any(|schema| schema.command == self.command && schema.events.contains(&event)),
            "{} does not declare event {event}",
            self.command
        );
        let line = serde_json::to_string(&Record(self.command, event, status, fields))?;
        let mut stdout = std::io::stdout().lock();
        writeln!(stdout, "{line}")?;
        stdout.flush()?;
        Ok(())
    }

    pub fn Diagnostic(&self, diagnostic: &SemanticDiagnostic) -> Result<()> {
        self.Emit("diagnostic", SeverityName(&diagnostic.severity), DiagnosticFields(diagnostic))
    }

    /// Run `body` and finish with exactly one `result` record. A failure record carries the error
    /// chain; the error is still returned so the process exits 1 with the human report on stderr.
    pub fn Run(self, body: impl FnOnce(&Records) -> Result<Value>) -> Result<()> {
        match body(&self) {
            Ok(fields) => self.Emit("result", "success", fields),
            Err(error) => {
                self.Emit("result", "failure", json!({ "error": format!("{error:#}") }))?;
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use miette::{NamedSource, SourceSpan};

    fn diagnostic(source: &str, offset: usize, length: usize) -> SemanticDiagnostic {
        SemanticDiagnostic {
            src: NamedSource::new("Src/Main.bd", source.to_owned()),
            span: SourceSpan::new(offset.into(), length),
            message: "unknown name `x`".into(),
            label: "here".into(),
            help: Some("declare `x`".into()),
            code: Some("E1001".into()),
            origin: None,
            severity: Severity::Error,
        }
    }

    #[test]
    fn v06_records_carry_common_schema_keys() {
        let record = Record("update", "change", "added", json!({ "package": "Local" }));
        let mut keys: Vec<&String> = record.as_object().unwrap().keys().collect();
        keys.sort();
        assert_eq!(keys, ["command", "event", "package", "schemaVersion", "status"]);
        assert_eq!(record["event"], "change");
        assert_eq!(record["status"], "added");
        assert_eq!(record["schemaVersion"], 1);
        assert_eq!(record["command"], "update");
        let line = serde_json::to_string(&record).unwrap();
        assert!(!line.contains('\n') && !line.contains('\u{1b}'), "{line}");
    }

    #[test]
    fn v06_every_json_command_ends_with_one_result_event() {
        for schema in OUTPUT_SCHEMAS {
            assert_eq!(schema.events.last(), Some(&"result"), "{}", schema.command);
            assert_eq!(schema.events.iter().filter(|event| **event == "result").count(), 1);
        }
        let commands: Vec<&str> = OUTPUT_SCHEMAS.iter().map(|schema| schema.command).collect();
        assert_eq!(commands, ["check", "build", "test", "add", "remove", "update", "doctor"]);
        for description in SchemaDescriptions() {
            assert_eq!(description["schemaVersion"], 1);
            assert_eq!(description["format"], "ndjson");
            assert!(description["events"].is_array());
        }
    }

    #[test]
    fn v06_diagnostic_span_reports_unicode_line_and_column() {
        let source = "i32 Main() {\n    caf\u{e9} x;\n}\n";
        let offset = source.find('x').unwrap();
        let fields = DiagnosticFields(&diagnostic(source, offset, 1));
        let record = &fields["diagnostic"];
        assert_eq!(record["code"], "E1001");
        assert_eq!(record["severity"], "error");
        assert_eq!(record["message"], "unknown name `x`");
        assert_eq!(record["span"]["file"], "Src/Main.bd");
        assert_eq!(record["span"]["offset"], offset);
        assert_eq!(record["span"]["length"], 1);
        assert_eq!(record["span"]["start"], json!({ "line": 2, "column": 10 }));
        assert_eq!(record["span"]["end"], json!({ "line": 2, "column": 11 }));
    }

    #[test]
    fn v06_diagnostic_span_outside_source_is_null() {
        let fields = DiagnosticFields(&diagnostic("abc", 10, 1));
        assert!(fields["diagnostic"]["span"].is_null());
    }

    #[test]
    fn v06_run_finishes_with_failure_result_and_returns_error() {
        let error = Records::new("check").Run(|_| anyhow::bail!("resolution failed")).unwrap_err();
        assert!(error.to_string().contains("resolution failed"));
    }
}
