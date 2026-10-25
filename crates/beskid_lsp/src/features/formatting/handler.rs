use tower_lsp_server::ls_types::{Range, TextEdit};

use crate::position::offset_range_to_lsp;

/// Full-document format of `text` with the one formatter `beskid fmt` uses.
///
/// Returns `None` (no edits) when the text does not parse or cannot be formatted safely; the
/// document is never changed in that case. An already formatted text gives an empty edit list.
pub fn handle_document_formatting(text: &str) -> Option<Vec<TextEdit>> {
    let formatted = beskid_analysis::format::format_source("<editor>", text).ok()?;
    if formatted == text {
        return Some(Vec::new());
    }
    Some(vec![full_document_edit(text, formatted)])
}

/// Range formatting formats the whole document (the formatter has one canonical output).
pub fn handle_range_formatting(text: &str, _range: Range) -> Option<Vec<TextEdit>> {
    handle_document_formatting(text)
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
        assert_eq!(handle_document_formatting(CANON), Some(Vec::new()), "formatted text needs 0 edits");
    }

    #[test]
    fn formatting_an_unparsable_document_returns_no_edits() {
        assert_eq!(handle_document_formatting("i32 Main() { let x = 5 return x; }\n"), None);
    }

    #[test]
    fn formatting_keeps_the_file_header_comment() {
        let edits =
            handle_document_formatting("// Header.\n// TODO: keep.\n\ni32 A() { return 1; }\n").expect("formats");
        assert!(edits[0].new_text.starts_with("// Header.\n// TODO: keep.\n\ni32 A()\n{\n"), "{}", edits[0].new_text);
    }
}
