use tower_lsp_server::ls_types::{InlayHint, InlayHintKind, InlayHintParams, Uri};

use crate::features::project_manifest::api as project_manifest;
use crate::position::offset_to_position;
use crate::session::store::Document;
use beskid_analysis::resolve::{Resolution, ResolvedType};

pub fn handle_inlay_hints(uri: &Uri, doc: &Document, _params: &InlayHintParams) -> Vec<InlayHint> {
    if project_manifest::is_manifest_uri(uri) {
        return Vec::new();
    }
    let Some(analysis) = doc.analysis.as_ref() else {
        return Vec::new();
    };
    let Some(resolution) = analysis.resolution.as_ref() else {
        return Vec::new();
    };

    let mut hints: Vec<InlayHint> = Vec::new();
    for (span, ty) in &resolution.tables.resolved_types {
        let label = format!(": {}", format_resolved_type(resolution, ty));
        hints.push(InlayHint {
            position: offset_to_position(&doc.text, span.end),
            label: label.into(),
            kind: Some(InlayHintKind::TYPE),
            text_edits: None,
            tooltip: None,
            padding_left: None,
            padding_right: Some(true),
            data: None,
        });
    }
    hints.sort_by(|a, b| {
        a.position
            .line
            .cmp(&b.position.line)
            .then_with(|| a.position.character.cmp(&b.position.character))
    });
    hints
}

fn format_resolved_type(resolution: &Resolution, ty: &ResolvedType) -> String {
    match ty {
        ResolvedType::Generic(name) => name.clone(),
        ResolvedType::Item(id) => resolution
            .items
            .get(id.0)
            .map(|item| item.name.clone())
            .unwrap_or_else(|| "?".to_string()),
    }
}
