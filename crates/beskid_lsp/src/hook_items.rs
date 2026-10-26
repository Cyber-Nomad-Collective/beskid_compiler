//! Generated hook items stay out of editor surfaces.
//!
//! Hook items are compiler-generated declarations (hook functions, hook argument and record types,
//! and the locals and parameters that carry records); the composition pass names them.

use beskid_analysis::composition::hooks::HOOK_ITEM_PREFIX;

/// Whether `name` is a generated hook item.
pub(crate) fn is_hook_item_name(name: &str) -> bool {
    beskid_analysis::composition::is_hook_item_name(name)
}

/// Whether rendered text (a hover body) names a hook item.
pub(crate) fn mentions_hook_item(text: &str) -> bool {
    text.contains(HOOK_ITEM_PREFIX)
}

#[cfg(test)]
mod tests {
    use tower_lsp_server::ls_types::{DocumentSymbolResponse, WorkspaceSymbolParams, WorkspaceSymbolResponse};

    use super::*;
    use crate::session::store::{Document, State, SyntaxHover, SyntaxSymbol};

    fn document(text: &str, names: &[&str]) -> Document {
        let mut offset = 0;
        let mut symbols = Vec::new();
        for name in names {
            symbols.push(SyntaxSymbol {
                name: (*name).to_string(),
                kind: beskid_analysis::services::AnalysisSymbolKind::Function,
                start: offset,
                end: offset + name.len(),
            });
            offset += name.len();
        }
        Document {
            version: 1,
            text: text.to_string(),
            syntax_definitions: Vec::new(),
            syntax_hovers: vec![
                SyntaxHover {
                    reference_start: 0,
                    reference_end: 4,
                    markdown: "```beskid\nunit __BeskidHookInit()\n```".into(),
                    location_path: "/p/Main.bd".into(),
                    location_start: 0,
                    location_end: 4,
                },
                SyntaxHover {
                    reference_start: 10,
                    reference_end: 14,
                    markdown: "```beskid\ni32 Main()\n```".into(),
                    location_path: "/p/Main.bd".into(),
                    location_start: 10,
                    location_end: 14,
                },
            ],
            syntax_symbols: symbols,
            bsol_semantic_token_candidates: Vec::new(),
            syntax_attribute_names: Vec::new(),
            syntax_completion: None,
            syntax_inlay_hints: Vec::new(),
            syntax_documentation: Vec::new(),
            syntax_diagnostics: Vec::new(),
            syntax_fixes: Vec::new(),
        }
    }

    #[test]
    fn prefix_decides_hook_names() {
        assert!(is_hook_item_name("__BeskidHookInit"));
        assert!(!is_hook_item_name("BeskidHook"));
        assert!(!is_hook_item_name("Main"));
    }

    #[test]
    fn hook_items_are_absent_from_document_and_workspace_symbols_and_hover() {
        let text = "__BeskidHookInit Main hello world";
        let doc = document(text, &["__BeskidHookInit", "Main"]);
        let uri = crate::workspace_scan::path_to_uri(&std::env::temp_dir().join("Main.bd")).expect("uri");

        let DocumentSymbolResponse::Nested(outline) =
            crate::features::document_symbols::handler::outline(&doc.text, &doc.syntax_symbols)
        else {
            panic!("nested outline");
        };
        assert_eq!(outline.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), vec!["Main"]);

        let mut state = State::default();
        state.docs.insert(uri.clone(), doc.clone());
        let WorkspaceSymbolResponse::Flat(found) =
            crate::features::workspace_symbols::handler::handle_workspace_symbols(
                &state,
                WorkspaceSymbolParams { query: String::new(), ..Default::default() },
            )
        else {
            panic!("flat workspace symbols");
        };
        assert_eq!(found.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), vec!["Main"]);

        assert!(crate::features::hover::handler::handle_hover(&uri, &doc, 2).is_none(), "hook hover is dropped");
        assert!(crate::features::hover::handler::handle_hover(&uri, &doc, 12).is_some(), "normal hover stays");
    }
}
