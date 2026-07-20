use std::path::Path;

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

    let Some(target) = doc.syntax_definitions.iter().find(|definition| {
        (definition.reference_start <= offset && offset <= definition.reference_end)
            || (entry_path.is_some_and(|path| path == definition.declaration_path)
                && definition.declaration_start <= offset
                && offset <= definition.declaration_end)
    }) else {
        return Vec::new();
    };
    let mut locations: Vec<Location> = doc
        .syntax_definitions
        .iter()
        .filter(|reference| {
            reference.declaration_path == target.declaration_path
                && reference.declaration_start == target.declaration_start
                && reference.declaration_end == target.declaration_end
        })
        .map(|reference| Location {
            uri: uri.clone(),
            range: offset_range_to_lsp(
                &doc.text,
                reference.reference_start,
                reference.reference_end,
            ),
        })
        .collect();
    if include_declaration {
        let Some(target_uri) = path_to_uri(&target.declaration_path) else {
            return locations;
        };
        locations.push(Location {
            uri: target_uri,
            range: symbol_location_to_lsp_range(
                &beskid_analysis::services::SymbolLocation {
                    path: target.declaration_path.clone(),
                    start: target.declaration_start,
                    end: target.declaration_end,
                },
                entry_path,
                &doc.text,
            ),
        });
    }
    locations.sort_by_key(|location| (location.uri.to_string(), location.range.start));
    locations.dedup();
    locations
}
