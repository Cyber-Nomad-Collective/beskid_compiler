use tower_lsp_server::ls_types::{GotoDefinitionResponse, Location, Uri};

use crate::features::project_manifest::api as project_manifest;
use crate::position::{offset_in_range, offset_range_to_lsp, symbol_location_to_lsp_range};
use crate::session::store::Document;
use crate::workspace_scan::{path_to_uri, uri_to_path};

/// Go-to-definition for Beskid sources or manifest dependency path targets.
pub fn handle_definition(
    uri: &Uri,
    doc: &Document,
    offset: usize,
) -> Option<GotoDefinitionResponse> {
    if project_manifest::is_manifest_uri(uri) {
        let location = project_manifest::dependency_path_location(uri, &doc.text, offset)?;
        return Some(GotoDefinitionResponse::Scalar(location));
    }

    let entry_path = uri_to_path(uri);
    if let Some(definition) = doc
        .syntax_definitions
        .iter()
        .filter(|definition| {
            definition.reference_start <= offset && offset <= definition.reference_end
        })
        .min_by_key(|definition| {
            definition
                .reference_end
                .saturating_sub(definition.reference_start)
        })
    {
        let target_uri = path_to_uri(&definition.declaration_path).unwrap_or_else(|| uri.clone());
        return Some(GotoDefinitionResponse::Scalar(Location {
            uri: target_uri,
            range: symbol_location_to_lsp_range(
                &beskid_analysis::services::SymbolLocation {
                    path: definition.declaration_path.clone(),
                    start: definition.declaration_start,
                    end: definition.declaration_end,
                },
                entry_path.as_deref(),
                &doc.text,
            ),
        }));
    }

    let analysis = doc.analysis.as_ref()?;
    let symbols = beskid_analysis::services::collect_document_symbols(analysis);
    symbols
        .iter()
        .find(|symbol| offset_in_range(offset, symbol.selection_start, symbol.selection_end))
        .map(|symbol| {
            GotoDefinitionResponse::Scalar(Location {
                uri: uri.clone(),
                range: offset_range_to_lsp(&doc.text, symbol.selection_start, symbol.selection_end),
            })
        })
}
