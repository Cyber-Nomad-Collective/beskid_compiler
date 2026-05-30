use tower_lsp_server::ls_types::{Hover, HoverContents, MarkupContent, MarkupKind, Uri};

use crate::commands::symbol_documentation::documentation_uri_for_document;
use crate::features::project_manifest::api as project_manifest;
use crate::position::{offset_in_range, offset_range_to_lsp, symbol_location_to_lsp_range};
use crate::session::store::Document;
use crate::workspace_scan::uri_to_path;

fn append_docs_link(markdown: String, doc: &Document, offset: usize) -> String {
    let Some(url) = documentation_uri_for_document(doc, offset) else {
        return markdown;
    };
    if markdown.contains("View documentation") {
        return markdown;
    }
    format!("{markdown}\n\n[View documentation]({url})")
}

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

    let analysis = doc.analysis.as_ref()?;
    let symbols = beskid_analysis::services::collect_document_symbols(analysis);
    if let Some(symbol) = symbols
        .iter()
        .find(|symbol| offset_in_range(offset, symbol.selection_start, symbol.selection_end))
    {
        let value = format!(
            "**{}** `{}`",
            beskid_analysis::services::symbol_kind_name(symbol.kind),
            symbol.name
        );
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: append_docs_link(value, doc, offset),
            }),
            range: Some(offset_range_to_lsp(
                &doc.text,
                symbol.selection_start,
                symbol.selection_end,
            )),
        });
    }

    let entry_path = uri_to_path(uri);
    let hover = beskid_analysis::services::hover_at_offset(analysis, offset)?;
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: append_docs_link(hover.markdown, doc, offset),
        }),
        range: Some(symbol_location_to_lsp_range(
            &hover.location,
            entry_path.as_deref(),
            &doc.text,
        )),
    })
}
