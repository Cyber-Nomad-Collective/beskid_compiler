use std::sync::Arc;

use beskid_analysis::format::FormatError;
use tower_lsp_server::ls_types::{MessageType, Range, TextEdit};

use crate::position::offset_range_to_lsp;
use crate::text_memo::TextMemo;

/// Full-document format of `text` with the one formatter `beskid fmt` uses.
///
/// Returns the reason when the text does not parse or cannot be formatted safely; the document
/// is never changed in that case. An already formatted text gives an empty edit list.
pub fn handle_document_formatting(text: &str) -> Result<Vec<TextEdit>, FormatError> {
    if let Some(Ok(formatted)) = FORMATTED.get(text) {
        return Ok(edits_to(text, formatted.as_deref()));
    }
    let outcome = beskid_analysis::format::format_source("<editor>", text);
    let remembered = match &outcome {
        Ok(formatted) => Ok((formatted != text).then(|| Arc::<str>::from(formatted.as_str()))),
        Err(_) => Err(()),
    };
    FORMATTED.insert(text, remembered);
    let formatted = outcome?;
    Ok(if formatted == text { Vec::new() } else { vec![full_document_edit(text, formatted)] })
}

/// The "Format document" edits of `text` for a code action: `None` when the text is formatted
/// already or does not parse. A text formatted once (by a code action or a formatting request) is
/// not formatted again.
pub fn document_formatting_for_action(text: &str) -> Option<Vec<TextEdit>> {
    let formatted = FORMATTED.get_or_compute(text, || match beskid_analysis::format::format_source("<editor>", text) {
        Ok(formatted) => Ok((formatted != text).then(|| Arc::<str>::from(formatted.as_str()))),
        Err(_) => Err(()),
    });
    formatted.ok().flatten().map(|formatted| edits_to(text, Some(&formatted)))
}

/// The formatted text of the last few texts: `Ok(None)` when a text is formatted already,
/// `Err(())` when it could not be formatted (a formatting request computes the reason again).
static FORMATTED: TextMemo<Result<Option<Arc<str>>, ()>> = TextMemo::new(8);

fn edits_to(text: &str, formatted: Option<&str>) -> Vec<TextEdit> {
    match formatted {
        Some(formatted) => vec![full_document_edit(text, formatted.to_string())],
        None => Vec::new(),
    }
}

/// Range formatting formats the whole document (the formatter has one canonical output).
pub fn handle_range_formatting(text: &str, _range: Range) -> Result<Vec<TextEdit>, FormatError> {
    handle_document_formatting(text)
}

/// How the editor is told that a document was not formatted, and why.
///
/// A parse error is already a diagnostic in the editor, so it goes to the log only. Any other
/// refusal is a formatter defect the user would not see otherwise: it is shown as a warning.
pub fn refusal_message(document: &str, error: &FormatError) -> (MessageType, String) {
    let message = format!("Beskid did not format {document}: {error}");
    match error {
        FormatError::Parse(_) => (MessageType::INFO, message),
        FormatError::Internal(_) => (MessageType::WARNING, message),
    }
}

fn full_document_edit(original: &str, replacement: String) -> TextEdit {
    TextEdit { range: offset_range_to_lsp(original, 0, original.len()), new_text: replacement }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp_server::ls_types::Position;

    struct Doc {
        text: String,
    }

    fn mk_doc(text: &str) -> Doc {
        Doc { text: text.to_string() }
    }

    #[test]
    fn formatting_returns_edit_for_non_canonical_source() {
        let doc = mk_doc("pub i32 Main() { return 42; }\n");
        let edits = handle_document_formatting(&doc.text).expect("parse + format");
        assert_eq!(edits.len(), 1);
        assert!(edits[0].new_text.contains("pub i32 Main()"));
        assert!(edits[0].new_text.contains("{\n    return 42;\n}"));
    }

    #[test]
    fn formatting_returns_empty_when_already_formatted() {
        let doc = mk_doc("pub i32 Main()\n{\n    return 42;\n}\n");
        let edits = handle_document_formatting(&doc.text).expect("parse + format");
        assert!(edits.is_empty());
    }

    #[test]
    fn range_formatting_uses_full_document_strategy() {
        let doc = mk_doc("pub i32 Main() { return 42; }\n");
        let range = Range::new(Position::new(0, 0), Position::new(0, 3));
        let edits = handle_range_formatting(&doc.text, range).expect("range format");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].range, offset_range_to_lsp(&doc.text, 0, doc.text.len()));
        assert!(edits[0].new_text.contains("pub i32 Main()"));
    }

    #[test]
    fn formatting_edit_reparses() {
        let doc = mk_doc("pub i32 Main() { return 42; }\n");
        let edits = handle_document_formatting(&doc.text).expect("format");
        assert_eq!(edits.len(), 1);
        let reparsed = beskid_analysis::services::parse_program(&edits[0].new_text);
        assert!(reparsed.is_ok(), "formatted edit must remain parseable");
    }

    /// The editor and `beskid fmt` use one function: the edit is exactly `format_source`'s output.
    #[test]
    fn formatting_matches_format_source_fixture_docs_and_control() {
        const MESSY: &str = include_str!("../../../../beskid_tests_surface/fixtures/format/docs_and_control.input.bd");
        const CANON: &str =
            include_str!("../../../../beskid_tests_surface/fixtures/format/docs_and_control.expected.bd");

        let from_api = beskid_analysis::format::format_source("<memory>", MESSY).expect("format_source");
        let doc = mk_doc(MESSY);
        let edits = handle_document_formatting(&doc.text).expect("lsp format");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, from_api, "LSP document format must match format_source output");
        assert_eq!(from_api, CANON, "fixture drift: update golden or formatter");
        assert_eq!(handle_document_formatting(CANON).expect("format"), Vec::new(), "formatted text needs 0 edits");
    }

    #[test]
    fn formatting_an_unparsable_document_returns_the_reason() {
        let error = handle_document_formatting("i32 Main() { let x = 5 return x; }\n").expect_err("parse error");
        assert!(matches!(error, FormatError::Parse(_)), "{error}");
        let (kind, message) = refusal_message("Main.bd", &error);
        assert_eq!(kind, MessageType::INFO, "a parse error is already a diagnostic");
        assert!(message.starts_with("Beskid did not format Main.bd: "), "{message}");
    }

    /// A code action and a formatting request of one text share one formatting, and the action
    /// offers no edit for a formatted or unparsable text.
    #[test]
    fn the_format_action_reuses_the_formatting_of_its_text() {
        let text = "pub i32 MemoMain() { return 42; }\n";
        let action = document_formatting_for_action(text).expect("an edit");
        assert!(super::FORMATTED.get(text).is_some(), "remembered for the text");
        assert_eq!(handle_document_formatting(text).expect("formats"), action);
        assert_eq!(document_formatting_for_action("pub i32 Main()\n{\n    return 42;\n}\n"), None);
        assert_eq!(document_formatting_for_action("i32 Main() { let x = 5 return x; }\n"), None);
        let error = handle_document_formatting("i32 Main() { let x = 5 return x; }\n").expect_err("still the reason");
        assert!(matches!(error, FormatError::Parse(_)), "{error}");
    }

    #[test]
    fn formatting_keeps_the_file_header_comment() {
        let edits =
            handle_document_formatting("// Header.\n// TODO: keep.\n\ni32 A() { return 1; }\n").expect("formats");
        assert!(edits[0].new_text.starts_with("// Header.\n// TODO: keep.\n\ni32 A()\n{\n"), "{}", edits[0].new_text);
    }
}
