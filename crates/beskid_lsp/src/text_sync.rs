use tower_lsp_server::ls_types::TextDocumentContentChangeEvent;

use crate::position::position_to_offset;

pub fn apply_document_changes(document: &mut String, changes: Vec<TextDocumentContentChangeEvent>) {
    for change in changes {
        if let Some(range) = change.range {
            let start = position_to_offset(document.as_str(), range.start);
            let end = position_to_offset(document.as_str(), range.end);
            let end = end.min(document.len());
            let start = start.min(end);
            document.replace_range(start..end, &change.text);
        } else {
            *document = change.text;
        }
    }
}

#[cfg(test)]
mod tests {
    use tower_lsp_server::ls_types::{Position, Range};

    use super::*;

    #[test]
    fn applies_incremental_replacement() {
        let mut doc = "hello world".to_string();
        apply_document_changes(
            &mut doc,
            vec![TextDocumentContentChangeEvent {
                range: Some(Range::new(Position::new(0, 0), Position::new(0, 5))),
                range_length: None,
                text: "goodbye".to_string(),
            }],
        );
        assert_eq!(doc, "goodbye world");
    }

    #[test]
    fn full_document_change_replaces_all() {
        let mut doc = "old".to_string();
        apply_document_changes(
            &mut doc,
            vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "new".to_string(),
            }],
        );
        assert_eq!(doc, "new");
    }
}
