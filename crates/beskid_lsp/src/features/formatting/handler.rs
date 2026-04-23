use tower_lsp_server::ls_types::{Range, TextEdit};

use crate::position::offset_range_to_lsp;
use crate::session::store::Document;

pub fn handle_document_formatting(document: &Document) -> Option<Vec<TextEdit>> {
    let parsed = beskid_analysis::services::parse_program(&document.text).ok()?;
    let formatted = beskid_analysis::format::format_program(&parsed).ok()?;
    if formatted == document.text {
        return Some(Vec::new());
    }
    Some(vec![full_document_edit(&document.text, formatted)])
}

pub fn handle_range_formatting(document: &Document, _range: Range) -> Option<Vec<TextEdit>> {
    // Current formatter emits canonical full-program output.
    // Range formatting is implemented as a full-document replacement strategy.
    handle_document_formatting(document)
}

fn full_document_edit(original: &str, replacement: String) -> TextEdit {
    TextEdit {
        range: offset_range_to_lsp(original, 0, original.len()),
        new_text: replacement,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp_server::ls_types::Position;

    use crate::session::store::Document;

    fn mk_doc(text: &str) -> Document {
        Document {
            version: 1,
            text: text.to_string(),
            text_hash: 0,
            analysis_cache_version: 0,
            analysis: None,
        }
    }

    #[test]
    fn formatting_returns_edit_for_non_canonical_source() {
        let doc = mk_doc("pub i32 main() { return 42; }\n");
        let edits = handle_document_formatting(&doc).expect("parse + format");
        assert_eq!(edits.len(), 1);
        assert!(edits[0].new_text.contains("pub i32 main()"));
        assert!(edits[0].new_text.contains("{\n    return 42;\n}"));
    }

    #[test]
    fn formatting_returns_empty_when_already_formatted() {
        let doc = mk_doc("pub i32 main()\n{\n    return 42;\n}\n");
        let edits = handle_document_formatting(&doc).expect("parse + format");
        assert!(edits.is_empty());
    }

    #[test]
    fn range_formatting_uses_full_document_strategy() {
        let doc = mk_doc("pub i32 main() { return 42; }\n");
        let range = Range::new(Position::new(0, 0), Position::new(0, 3));
        let edits = handle_range_formatting(&doc, range).expect("range format");
        assert_eq!(edits.len(), 1);
        assert_eq!(
            edits[0].range,
            offset_range_to_lsp(&doc.text, 0, doc.text.len())
        );
        assert!(edits[0].new_text.contains("pub i32 main()"));
    }

    #[test]
    fn formatting_edit_reparses() {
        let doc = mk_doc("pub i32 main() { return 42; }\n");
        let edits = handle_document_formatting(&doc).expect("format");
        assert_eq!(edits.len(), 1);
        let reparsed = beskid_analysis::services::parse_program(&edits[0].new_text);
        assert!(reparsed.is_ok(), "formatted edit must remain parseable");
    }

    /// LSP handler must agree with `beskid_analysis::format::format_program` (same as CLI).
    #[test]
    fn formatting_matches_format_program_fixture_docs_and_control() {
        const MESSY: &str =
            include_str!("../../../../beskid_tests/fixtures/format/docs_and_control.input.bd");
        const CANON: &str =
            include_str!("../../../../beskid_tests/fixtures/format/docs_and_control.expected.bd");

        let parsed = beskid_analysis::services::parse_program(MESSY).expect("fixture parses");
        let from_api = beskid_analysis::format::format_program(&parsed).expect("format_program");

        let doc = mk_doc(MESSY);
        let edits = handle_document_formatting(&doc).expect("lsp format");
        assert_eq!(edits.len(), 1);
        assert_eq!(
            edits[0].new_text, from_api,
            "LSP document format must match format_program output"
        );
        assert_eq!(from_api, CANON, "fixture drift: update golden or formatter");
    }
}
