use std::collections::HashMap;

use tower_lsp_server::ls_types::{Position, PrepareRenameResponse, TextEdit, Uri, WorkspaceEdit};

use crate::features::project_manifest::api as project_manifest;
use crate::position::{offset_range_to_lsp, position_to_offset};
use crate::session::store::Document;

/// Valid rename range for the identifier at `offset` (manifest tokens or resolved Beskid refs).
pub fn handle_prepare_rename(
    uri: &Uri,
    doc: &Document,
    offset: usize,
) -> Option<PrepareRenameResponse> {
    let (start, end) = if project_manifest::is_manifest_uri(uri) {
        token_span_at_offset(&doc.text, offset)?
    } else {
        // Rename is only supported for resolved references.
        doc.analysis.as_ref().and_then(|analysis| {
            beskid_analysis::services::definition_at_offset(analysis, offset)
        })?;
        token_span_at_offset(&doc.text, offset)?
    };

    Some(PrepareRenameResponse::Range(offset_range_to_lsp(
        &doc.text, start, end,
    )))
}

/// Produce a workspace edit renaming the symbol at `position` when the new name is a valid identifier.
pub fn handle_rename(
    uri: &Uri,
    doc: &Document,
    position: Position,
    new_name: &str,
) -> Option<WorkspaceEdit> {
    if !is_valid_identifier(new_name) {
        return None;
    }

    let offset = position_to_offset(&doc.text, position);
    let mut ranges: Vec<(usize, usize)> = if project_manifest::is_manifest_uri(uri) {
        project_manifest::token_references(&doc.text, offset)
    } else {
        let analysis = doc.analysis.as_ref()?;
        beskid_analysis::services::references_at_offset(analysis, offset, true)
            .into_iter()
            .map(|reference| (reference.location.start, reference.location.end))
            .collect()
    };

    if ranges.is_empty() {
        return None;
    }

    ranges.sort_unstable();
    ranges.dedup();

    let edits: Vec<TextEdit> = ranges
        .into_iter()
        .map(|(start, end)| TextEdit {
            range: offset_range_to_lsp(&doc.text, start, end),
            new_text: new_name.to_string(),
        })
        .collect();

    let mut changes = HashMap::new();
    changes.insert(uri.clone(), edits);

    Some(WorkspaceEdit {
        changes: Some(changes),
        ..WorkspaceEdit::default()
    })
}

fn token_span_at_offset(text: &str, offset: usize) -> Option<(usize, usize)> {
    if text.is_empty() {
        return None;
    }

    let bytes = text.as_bytes();
    let mut start = offset.min(text.len());
    if start == text.len() {
        start = start.saturating_sub(1);
    }

    if !is_ident_byte(*bytes.get(start)?) && start > 0 && is_ident_byte(*bytes.get(start - 1)?) {
        start -= 1;
    }
    if !is_ident_byte(*bytes.get(start)?) {
        return None;
    }

    let mut left = start;
    while left > 0 && is_ident_byte(*bytes.get(left - 1)?) {
        left -= 1;
    }

    let mut right = start + 1;
    while right < text.len() && is_ident_byte(*bytes.get(right)?) {
        right += 1;
    }

    Some((left, right))
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;
    use crate::session::lifecycle::build_document;
    use crate::workspace_scan::path_to_uri;

    fn source() -> &'static str {
        "i32 add(i32 lhs, i32 rhs) {\n    return lhs + rhs;\n}\n\ni32 main() {\n    return add(1, 2);\n}\n"
    }

    fn project_fixture() -> (TempDir, Uri) {
        let root = tempfile::tempdir().expect("temporary project");
        let source_dir = root.path().join("Src");
        fs::create_dir_all(&source_dir).expect("source directory");
        fs::write(source_dir.join("Main.bd"), source()).expect("source file");
        fs::write(
            root.path().join("RenameProject.bproj"),
            r#"RenameProject {
  name = "RenameProject"
  version = "0.1.0"
}

target "App" {
  kind = "App"
  entry = "Main.bd"
}
"#,
        )
        .expect("project manifest");
        let uri = path_to_uri(&source_dir.join("Main.bd")).expect("file uri");
        (root, uri)
    }

    #[tokio::test]
    async fn prepare_rename_for_resolved_symbol_returns_selection() {
        let state = tokio::sync::RwLock::new(crate::session::store::State::default());
        state.read().await.mark_initial_scan_complete();
        let (_root, uri) = project_fixture();
        let doc = build_document(&state, &uri, 1, source().to_string()).await;
        let offset = source().find("lhs +").expect("lhs");
        let response = handle_prepare_rename(&uri, &doc, offset).expect("prepare rename");
        let PrepareRenameResponse::Range(range) = response else {
            panic!("expected range response");
        };
        assert_eq!(range.start.line, 1);
    }

    #[tokio::test]
    async fn rename_updates_definition_and_references() {
        let state = tokio::sync::RwLock::new(crate::session::store::State::default());
        state.read().await.mark_initial_scan_complete();
        let (_root, uri) = project_fixture();
        let doc = build_document(&state, &uri, 1, source().to_string()).await;
        let position = Position::new(1, 11);
        let edit = handle_rename(&uri, &doc, position, "left").expect("workspace edit");
        let changes = edit.changes.expect("changes map");
        let edits = changes.get(&uri).expect("uri edits");
        assert!(
            edits.len() >= 2,
            "rename should include declaration and references"
        );
        assert!(edits.iter().all(|item| item.new_text == "left"));
    }
}
