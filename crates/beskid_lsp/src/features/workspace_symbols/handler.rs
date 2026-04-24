use tower_lsp_server::ls_types::{Location, SymbolInformation, WorkspaceSymbolParams, WorkspaceSymbolResponse};

use crate::adapters::symbol::analysis_symbol_kind_to_lsp;
use crate::features::project_manifest::api as project_manifest;
use crate::position::offset_range_to_lsp;
use crate::session::store::State;

pub fn handle_workspace_symbols(state: &State, params: WorkspaceSymbolParams) -> WorkspaceSymbolResponse {
    let query = params.query.to_lowercase();
    let mut out: Vec<SymbolInformation> = Vec::new();

    for (uri, doc) in state.docs.iter().chain(state.workspace_index.iter()) {
        if project_manifest::is_manifest_uri(uri) {
            continue;
        }
        let Some(analysis) = doc.analysis.as_ref() else {
            continue;
        };
        for sym in beskid_analysis::services::collect_document_symbols(analysis) {
            if !query.is_empty() && !sym.name.to_lowercase().contains(&query) {
                continue;
            }
            let range =
                offset_range_to_lsp(&doc.text, sym.selection_start, sym.selection_end);
            #[allow(deprecated)]
            out.push(SymbolInformation {
                name: sym.name,
                kind: analysis_symbol_kind_to_lsp(sym.kind),
                tags: None,
                deprecated: None,
                location: Location {
                    uri: uri.clone(),
                    range,
                },
                container_name: None,
            });
        }
    }

    WorkspaceSymbolResponse::from(out)
}
