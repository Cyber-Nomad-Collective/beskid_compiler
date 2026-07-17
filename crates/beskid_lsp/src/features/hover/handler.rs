use tower_lsp_server::ls_types::{Hover, HoverContents, MarkupContent, MarkupKind, Uri};

use crate::features::project_manifest::api as project_manifest;
use crate::position::symbol_location_to_lsp_range;
use crate::session::store::Document;
use crate::workspace_scan::uri_to_path;

/// Markdown hover for symbols, types, or manifest tokens at `offset`.
pub fn handle_hover(uri: &Uri, doc: &Document, offset: usize) -> Option<Hover> {
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
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: hover.markdown.clone(),
        }),
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
