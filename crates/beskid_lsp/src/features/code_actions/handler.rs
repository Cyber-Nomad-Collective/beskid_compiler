use std::collections::HashMap;

use beskid_analysis::doc::{DocCommentEdit, doc_comment_edit_for_offset};
use tower_lsp_server::ls_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, CodeActionResponse,
    Diagnostic, NumberOrString, TextEdit, Uri, WorkspaceEdit,
};

use crate::features::formatting;
use crate::features::project_manifest::api as project_manifest;
use crate::position::{offset_range_to_lsp, position_to_offset};
use crate::session::store::Document;

fn doc_comment_code_action(
    uri: &Uri,
    doc: &Document,
    offset: usize,
    title: &'static str,
    diagnostics: Option<Vec<Diagnostic>>,
) -> Option<CodeAction> {
    let snap = doc.analysis.as_ref()?;
    let res = snap.resolution.as_ref()?;
    let edit = doc_comment_edit_for_offset(&snap.program.node, res, &doc.text, offset)?;
    let (range, new_text) = match edit {
        DocCommentEdit::Insert { at, text } => (offset_range_to_lsp(&doc.text, at, at), text),
        DocCommentEdit::Replace { start, end, text } => {
            (offset_range_to_lsp(&doc.text, start, end), text)
        }
    };
    let mut changes = HashMap::new();
    changes.insert(uri.clone(), vec![TextEdit { range, new_text }]);
    Some(CodeAction {
        title: title.to_string(),
        kind: Some(if diagnostics.is_some() {
            CodeActionKind::QUICKFIX
        } else {
            CodeActionKind::SOURCE
        }),
        diagnostics,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            ..WorkspaceEdit::default()
        }),
        ..CodeAction::default()
    })
}

/// Quick fixes (e.g. doc comments), manifest assists, and source actions such as format-document.
pub fn handle_code_actions(
    uri: &Uri,
    doc: &Document,
    params: &CodeActionParams,
) -> CodeActionResponse {
    let mut actions: Vec<CodeActionOrCommand> = Vec::new();

    if !project_manifest::is_manifest_uri(uri) {
        if let Some(edits) = formatting::handler::handle_document_formatting(doc) {
            let mut changes = HashMap::new();
            changes.insert(uri.clone(), edits);
            actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                title: "Format document".to_string(),
                kind: Some(CodeActionKind::SOURCE),
                edit: Some(WorkspaceEdit {
                    changes: Some(changes),
                    ..WorkspaceEdit::default()
                }),
                ..CodeAction::default()
            }));
        }

        for diag in &params.context.diagnostics {
            if let Some(NumberOrString::String(code)) = &diag.code {
                if code == "W1503"
                    && let Some(action) = remove_lines_action(uri, doc, diag)
                {
                    actions.push(CodeActionOrCommand::CodeAction(action));
                }
                if matches!(
                    code.as_str(),
                    "W1610" | "W1611" | "W1612" | "W1613" | "W1614" | "W1615" | "W1620"
                        | "W1621" | "W1622" | "W1623" | "W1624" | "W1625"
                ) && let Some(action) = doc_comment_code_action(
                    uri,
                    doc,
                    position_to_offset(&doc.text, diag.range.start),
                    "Update documentation comment",
                    Some(vec![diag.clone()]),
                ) {
                    actions.push(CodeActionOrCommand::CodeAction(action));
                }
            }
        }

        if let Some(path) = uri.to_file_path() {
            if path.extension().and_then(|e| e.to_str()) == Some("bd") {
                let offset = position_to_offset(&doc.text, params.range.start);
                if let Some(action) = doc_comment_code_action(
                    uri,
                    doc,
                    offset,
                    "Generate or update documentation comment",
                    None,
                ) {
                    actions.push(CodeActionOrCommand::CodeAction(action));
                }
            }
        }
    }

    CodeActionResponse::from(actions)
}

fn remove_lines_action(uri: &Uri, doc: &Document, diag: &Diagnostic) -> Option<CodeAction> {
    let start = position_to_offset(&doc.text, diag.range.start);
    let end = position_to_offset(&doc.text, diag.range.end);
    let (line_start, line_end) = line_span(&doc.text, start, end);
    let range = offset_range_to_lsp(&doc.text, line_start, line_end);
    let mut changes = HashMap::new();
    changes.insert(
        uri.clone(),
        vec![TextEdit {
            range,
            new_text: String::new(),
        }],
    );
    Some(CodeAction {
        title: "Remove unused import".to_string(),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diag.clone()]),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            ..WorkspaceEdit::default()
        }),
        ..CodeAction::default()
    })
}

fn line_span(source: &str, start_off: usize, end_off: usize) -> (usize, usize) {
    let start_off = start_off.min(source.len());
    let end_off = end_off.min(source.len()).max(start_off);
    let line_start = source[..start_off].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = if let Some(rel) = source[end_off..].find('\n') {
        end_off + rel + 1
    } else {
        source.len()
    };
    (line_start, line_end)
}
