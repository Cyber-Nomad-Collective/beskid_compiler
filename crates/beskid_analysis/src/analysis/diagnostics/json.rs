//! `--error-format=json`: one JSON object per diagnostic, modelled on rustc's JSON output.
//!
//! The schema is documented in `docs/diagnostics-json.md` of the compiler. Consumers must tolerate
//! new fields and new enum values; `schema` changes only when a field changes meaning or goes away.

use serde_json::{Value, json};

use super::{SemanticDiagnostic, line_column};
use crate::analysis::diagnostic_codes;

/// Version of the JSON diagnostic schema.
pub const SCHEMA_VERSION: u64 = 1;

fn span_json(
    file: &str,
    text: &str,
    start: usize,
    end: usize,
    is_primary: bool,
    label: Option<&str>,
    replacement: Option<(&str, &str)>,
) -> Value {
    let start = start.min(text.len());
    let end = end.clamp(start, text.len());
    let (line_start, column_start) = line_column(text, start);
    let (line_end, column_end) = line_column(text, end);
    json!({
        "file_name": file,
        "byte_start": start,
        "byte_end": end,
        "line_start": line_start,
        "line_end": line_end,
        "column_start": column_start,
        "column_end": column_end,
        "is_primary": is_primary,
        "label": label,
        "suggested_replacement": replacement.map(|(text, _)| text),
        "suggestion_applicability": replacement.map(|(_, applicability)| applicability),
    })
}

fn child(message: &str, spans: Vec<Value>) -> Value {
    json!({
        "message": message,
        "code": null,
        "level": "help",
        "spans": spans,
        "children": [],
        "rendered": null,
    })
}

/// The JSON object for `diagnostic`. `rendered` is the human drawing of the same diagnostic.
pub fn diagnostic_json(diagnostic: &SemanticDiagnostic, rendered: Option<&str>) -> Value {
    let file = diagnostic.file_name();
    let text = diagnostic.source_text();
    let (start, end) = diagnostic.range();
    let mut spans = vec![span_json(file, text, start, end, true, Some(&diagnostic.label), None)];
    for related in &diagnostic.related {
        let (start, end) = (related.span.offset(), related.span.offset() + related.span.len());
        spans.push(span_json(
            related.file_name(file),
            related.file_text(text),
            start,
            end,
            false,
            Some(&related.label),
            None,
        ));
    }
    let mut children = Vec::new();
    if let Some(help) = &diagnostic.help {
        children.push(child(help, Vec::new()));
    }
    for fix in &diagnostic.fixes {
        let spans = fix
            .edits
            .iter()
            .map(|edit| {
                let (start, end) = edit.range();
                span_json(file, text, start, end, true, None, Some((&edit.replacement, fix.applicability.as_str())))
            })
            .collect();
        children.push(child(&fix.title, spans));
    }
    let code = diagnostic.code.as_deref().map(|code| {
        json!({ "code": code, "explanation": null, "url": diagnostic_codes::docs_url(code) })
    });
    json!({
        "$message_type": "diagnostic",
        "schema": SCHEMA_VERSION,
        "message": diagnostic.message,
        "code": code,
        "level": diagnostic.severity.as_str(),
        "origin": diagnostic.origin,
        "spans": spans,
        "children": children,
        "tags": diagnostic.tags.iter().map(|tag| tag.as_str()).collect::<Vec<_>>(),
        "rendered": rendered,
    })
}

/// The JSON object for a failure that carries no structured diagnostic (no code, no spans).
pub fn message_json(message: &str, rendered: Option<&str>) -> Value {
    json!({
        "$message_type": "diagnostic",
        "schema": SCHEMA_VERSION,
        "message": message,
        "code": null,
        "level": "error",
        "origin": null,
        "spans": [],
        "children": [],
        "tags": [],
        "rendered": rendered,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::diagnostics::{Applicability, Fix, FixEdit, Severity, make_diagnostic};
    use crate::syntax::SpanInfo;

    fn span(start: usize, end: usize) -> SpanInfo {
        SpanInfo { start, end, line_col_start: (1, 1), line_col_end: (1, 1) }
    }

    #[test]
    fn diagnostic_object_follows_the_documented_schema() {
        let source = "use A.B;\ni32 Main() {\n    return 0;\n}\n";
        let diagnostic = make_diagnostic(
            "Src/Main.bd",
            source,
            span(4, 7),
            "Unused import `A.B`.",
            "never used",
            Some("Remove the import.".into()),
            Some("W1503".into()),
            Severity::Warning,
        )
        .with_related(span(13, 17), "entry point")
        .with_fix(Fix::new("Remove the import", Applicability::MachineApplicable, vec![FixEdit::new(span(0, 9), "")]));
        let value = diagnostic_json(&diagnostic, Some("rendered text"));

        assert_eq!(value["$message_type"], "diagnostic");
        assert_eq!(value["schema"], 1);
        assert_eq!(value["level"], "warning");
        assert_eq!(value["code"]["code"], "W1503");
        assert_eq!(value["code"]["url"], diagnostic_codes::REGISTRY_URL);
        assert_eq!(value["tags"], json!(["unnecessary"]));
        assert_eq!(value["rendered"], "rendered text");
        let primary = &value["spans"][0];
        assert_eq!(primary["file_name"], "Src/Main.bd");
        assert_eq!((primary["byte_start"].as_u64(), primary["byte_end"].as_u64()), (Some(4), Some(7)));
        assert_eq!((primary["line_start"].as_u64(), primary["column_start"].as_u64()), (Some(1), Some(5)));
        assert_eq!(primary["is_primary"], true);
        assert_eq!(primary["label"], "never used");
        let related = &value["spans"][1];
        assert_eq!(related["is_primary"], false);
        assert_eq!(related["label"], "entry point");
        assert_eq!(related["line_start"], 2);
        assert_eq!(value["children"][0]["level"], "help");
        assert_eq!(value["children"][0]["message"], "Remove the import.");
        let fix = &value["children"][1];
        assert_eq!(fix["message"], "Remove the import");
        assert_eq!(fix["spans"][0]["suggested_replacement"], "");
        assert_eq!(fix["spans"][0]["suggestion_applicability"], "MachineApplicable");
        let line = serde_json::to_string(&value).unwrap();
        assert!(!line.contains('\n'), "one object per line");
    }

    #[test]
    fn unstructured_failure_has_no_code_or_spans() {
        let value = message_json("cannot read the manifest", None);
        assert_eq!(value["code"], Value::Null);
        assert_eq!(value["spans"], json!([]));
        assert_eq!(value["level"], "error");
    }
}
