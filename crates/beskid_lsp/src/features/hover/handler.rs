use tower_lsp_server::ls_types::{Hover, HoverContents, MarkupContent, MarkupKind, Uri};

use crate::features::project_manifest::api as project_manifest;
use crate::manifest_uri::is_standalone_bsol_uri;
use crate::position::symbol_location_to_lsp_range;
use crate::session::store::Document;
use crate::workspace_scan::uri_to_path;

/// Markdown hover for symbols, types, or manifest tokens at `offset`.
pub fn handle_hover(uri: &Uri, doc: &Document, offset: usize) -> Option<Hover> {
    if is_standalone_bsol_uri(uri) {
        return crate::standalone_bsol::hover(&doc.text, offset);
    }
    if project_manifest::is_manifest_uri(uri) {
        if let Some(token) = project_manifest::token_at_offset(&doc.text, offset)
            && let Some(message) = project_manifest::hover_markdown(token)
        {
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: message.to_string(),
                }),
                range: None,
            });
        }
        return None;
    }

    let hover = doc
        .syntax_hovers
        .iter()
        .filter(|hover| hover.reference_start <= offset && offset <= hover.reference_end)
        .min_by_key(|hover| hover.reference_end.saturating_sub(hover.reference_start))?;
    let entry_path = uri_to_path(uri);
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent { kind: MarkupKind::Markdown, value: hover.markdown.clone() }),
        range: Some(symbol_location_to_lsp_range(
            &beskid_analysis::services::SymbolLocation {
                path: hover.location_path.clone(),
                start: hover.location_start,
                end: hover.location_end,
            },
            entry_path.as_deref(),
            &doc.text,
        )),
    })
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use tower_lsp_server::ls_types::{HoverContents, Uri};

    use super::handle_hover;
    use crate::session::store::Document;

    #[test]
    fn standalone_bsol_hover_describes_the_schemaless_escape_hatch() {
        let source = "payload @schemaless { raw }";
        let doc = Document {
            version: 1,
            text: source.to_string(),
            syntax_definitions: Vec::new(),
            syntax_hovers: Vec::new(),
            syntax_symbols: Vec::new(),
            syntax_completion: None,
            syntax_inlay_hints: Vec::new(),
            syntax_documentation: Vec::new(),
            syntax_diagnostics: Vec::new(),
            syntax_fixes: Vec::new(),
        };
        let offset = source.find("schemaless").expect("marker");

        let hover = handle_hover(&Uri::from_str("file:///standalone/schema.bsol").expect("uri"), &doc, offset)
            .expect("standalone BSOL marker hover");

        let HoverContents::Markup(contents) = hover.contents else {
            panic!("expected markdown hover");
        };
        assert!(contents.value.contains("raw text"), "unexpected standalone BSOL hover: {contents:#?}");
    }
}
