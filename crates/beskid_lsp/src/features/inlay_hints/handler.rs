use tower_lsp_server::ls_types::{InlayHint, InlayHintKind, InlayHintParams, Uri};

use crate::features::project_manifest::api as project_manifest;
use crate::position::offset_to_position;
use crate::session::store::Document;

/// Type hints proven by generation-safe syntax facts for the current document revision.
pub fn handle_inlay_hints(uri: &Uri, doc: &Document, _params: &InlayHintParams) -> Vec<InlayHint> {
    if project_manifest::is_manifest_uri(uri) {
        return Vec::new();
    }
    let mut hints: Vec<InlayHint> = doc
        .syntax_inlay_hints
        .iter()
        .map(|hint| InlayHint {
            position: offset_to_position(&doc.text, hint.end),
            label: format!(": {}", hint.type_label).into(),
            kind: Some(InlayHintKind::TYPE),
            text_edits: None,
            tooltip: None,
            padding_left: None,
            padding_right: Some(true),
            data: None,
        })
        .collect();
    hints.sort_by(|a, b| {
        a.position.line.cmp(&b.position.line).then_with(|| a.position.character.cmp(&b.position.character))
    });
    hints
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use tower_lsp_server::ls_types::{InlayHintLabel, InlayHintParams, Uri};

    use super::handle_inlay_hints;
    use crate::session::store::{Document, SyntaxInlayHint};

    #[test]
    fn syntax_inlay_hints_work_without_legacy_analysis_snapshot() {
        let doc = Document {
            version: 1,
            text: "i32 Main() { let value = 1; }".to_string(),
            syntax_definitions: Vec::new(),
            syntax_hovers: Vec::new(),
            syntax_symbols: Vec::new(),
            syntax_completion: None,
            syntax_inlay_hints: vec![SyntaxInlayHint { start: 25, end: 30, type_label: "i32".to_string() }],
            syntax_documentation: Vec::new(),
            syntax_diagnostics: Vec::new(),
        };
        let uri = Uri::from_str("file:///tmp/inlay.bd").expect("URI");
        let params: InlayHintParams = serde_json::from_value(serde_json::json!({
            "textDocument": { "uri": uri },
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 0, "character": 32 }
            }
        }))
        .expect("valid inlay hint parameters");
        let hints = handle_inlay_hints(&uri, &doc, &params);

        assert_eq!(hints.len(), 1);
        assert!(matches!(&hints[0].label, InlayHintLabel::String(label) if label == ": i32"));
    }
}
