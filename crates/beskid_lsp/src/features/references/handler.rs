use std::path::Path;

use beskid_analysis::CompilationContext;
use tower_lsp_server::ls_types::{Location, Uri};

use crate::features::project_manifest::api as project_manifest;
use crate::position::{offset_range_to_lsp, symbol_location_to_lsp_range};
use crate::session::store::Document;
use crate::workspace_scan::path_to_uri;

/// Find references (optionally including the declaration) for manifest tokens or Beskid symbols.
pub fn handle_references(
    uri: &Uri,
    doc: &Document,
    offset: usize,
    include_declaration: bool,
    entry_path: Option<&Path>,
    mut compilation_context: Option<CompilationContext>,
) -> Vec<Location> {
    if project_manifest::is_manifest_uri(uri) {
        return project_manifest::token_references(&doc.text, offset)
            .into_iter()
            .map(|(start, end)| Location {
                uri: uri.clone(),
                range: offset_range_to_lsp(&doc.text, start, end),
            })
            .collect();
    }

    let analysis = match doc.analysis.as_ref() {
        Some(analysis) => analysis,
        None => return Vec::new(),
    };

    let references = if let (Some(path), Some(ctx)) = (entry_path, compilation_context.as_mut()) {
        if ctx.compile_plan.is_some() {
            if let Some(assembly) = ctx.assembly_for_entry(path, &doc.text) {
                beskid_analysis::services::references_at_offset_workspace(
                    analysis,
                    assembly,
                    path,
                    offset,
                    include_declaration,
                )
            } else {
                Vec::new()
            }
        } else if let Some(assembly) = ctx.assembly_for_entry(path, &doc.text) {
            beskid_analysis::services::references_at_offset_workspace(
                analysis,
                assembly,
                path,
                offset,
                include_declaration,
            )
        } else {
            beskid_analysis::services::references_at_offset(analysis, offset, include_declaration)
        }
    } else {
        beskid_analysis::services::references_at_offset(analysis, offset, include_declaration)
    };

    references
        .into_iter()
        .filter_map(|reference| {
            let target_uri = path_to_uri(&reference.location.path)?;
            Some(Location {
                uri: target_uri,
                range: symbol_location_to_lsp_range(
                    &reference.location,
                    entry_path,
                    &doc.text,
                ),
            })
        })
        .collect()
}
