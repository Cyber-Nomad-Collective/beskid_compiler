use serde_json::json;
use tower_lsp_server::ls_types::{DocumentSymbol, DocumentSymbolResponse, Range, SymbolKind, SymbolTag, Uri};

use crate::adapters::symbol::analysis_symbol_kind_to_lsp;
use crate::features::project_manifest::api as project_manifest;
use crate::manifest_uri::is_standalone_bsol_uri;
use crate::position::offset_range_to_lsp;
use crate::session::store::SyntaxSymbol;

fn build_document_symbol(
    name: String,
    detail: Option<String>,
    kind: SymbolKind,
    tags: Option<Vec<SymbolTag>>,
    range: Range,
    selection_range: Range,
) -> DocumentSymbol {
    serde_json::from_value(json!({
        "name": name,
        "detail": detail,
        "kind": kind,
        "tags": tags,
        "range": range,
        "selectionRange": selection_range,
        "children": null
    }))
    .expect("valid DocumentSymbol payload")
}

/// The outline of `text`, parsed on request. The outline is syntax only, so it never waits for
/// the analysis of an earlier edit. `None` when the text does not parse and no recovery of it
/// is remembered yet: the outline never starts a parse recovery (a large broken buffer costs
/// seconds of candidate parses), the caller answers from the last analyzed symbols instead.
pub fn document_symbols_for_text(uri: &Uri, text: &str) -> Option<DocumentSymbolResponse> {
    if project_manifest::is_manifest_uri(uri) {
        return Some(DocumentSymbolResponse::Nested(project_manifest::document_symbols(uri, text)));
    }
    if is_standalone_bsol_uri(uri) {
        return Some(DocumentSymbolResponse::Nested(Vec::new()));
    }
    let parsed = beskid_analysis::services::parse_program_without_recovery(uri.as_str(), text)?;
    let symbols = crate::session::lifecycle::syntax_symbols_for_program(&parsed.program);
    Some(outline(text, &symbols))
}

/// Outline entries for `symbols` of `text`; compiler-generated hook items are left out.
pub(crate) fn outline(text: &str, symbols: &[SyntaxSymbol]) -> DocumentSymbolResponse {
    let mapped = symbols
        .iter()
        .filter(|symbol| !crate::hook_items::is_hook_item_name(&symbol.name))
        .map(|symbol| {
            let range = offset_range_to_lsp(text, symbol.start, symbol.end);
            build_document_symbol(
                symbol.name.clone(),
                Some(beskid_analysis::services::symbol_kind_name(symbol.kind).to_string()),
                analysis_symbol_kind_to_lsp(symbol.kind),
                Option::<Vec<SymbolTag>>::None,
                range,
                range,
            )
        })
        .collect();

    DocumentSymbolResponse::Nested(mapped)
}
