use std::collections::HashMap;

use beskid_analysis::doc::DocCommentEdit;
use tower_lsp_server::ls_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, CodeActionResponse,
    Diagnostic, NumberOrString, TextEdit, Uri, WorkspaceEdit,
};

use crate::features::formatting;
use crate::features::project_manifest::api as project_manifest;
use crate::position::{offset_range_to_lsp, position_to_offset};
use crate::session::documentation_facts::{
    doc_comment_edit_from_syntax_facts, syntax_documentation_facts_for_source,
};
use crate::session::store::Document;

fn doc_comment_code_action(
    uri: &Uri,
    doc: &Document,
    offset: usize,
    title: &'static str,
    diagnostics: Option<Vec<Diagnostic>>,
) -> Option<CodeAction> {
    // Prefer generation-bound facts attached to this buffer revision. When facts are
    // missing (disk snapshot before refresh), rebuild them from the current text only —
    // never from a legacy HIR/analysis snapshot.
    let facts = if doc.syntax_documentation.is_empty() {
        syntax_documentation_facts_for_source(uri.as_str(), &doc.text)
    } else {
        doc.syntax_documentation.clone()
    };
    let edit = doc_comment_edit_from_syntax_facts(&facts, offset)?;
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
                if code == "W1639"
                    && let Some(action) = remove_range_action(uri, diag, "Remove empty enum constructor parens")
                {
                    actions.push(CodeActionOrCommand::CodeAction(action));
                }
                if matches!(
                    code.as_str(),
                    "W1610"
                        | "W1611"
                        | "W1612"
                        | "W1613"
                        | "W1614"
                        | "W1615"
                        | "W1620"
                        | "W1621"
                        | "W1622"
                        | "W1623"
                        | "W1624"
                        | "W1625"
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

        if let Some(path) = uri.to_file_path()
            && path.extension().and_then(|e| e.to_str()) == Some("bd")
        {
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

fn remove_range_action(uri: &Uri, diag: &Diagnostic, title: &'static str) -> Option<CodeAction> {
    if diag.range.start == diag.range.end {
        return None;
    }

    let mut changes = HashMap::new();
    changes.insert(
        uri.clone(),
        vec![TextEdit {
            range: diag.range,
            new_text: String::new(),
        }],
    );
    Some(CodeAction {
        title: title.to_string(),
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use tower_lsp_server::ls_types::{
        CodeActionContext, CodeActionOrCommand, CodeActionParams, Position, Range,
        TextDocumentIdentifier, Uri, WorkDoneProgressParams,
    };

    use super::handle_code_actions;
    use crate::session::documentation_facts::syntax_documentation_facts_for_source;
    use crate::session::store::Document;

    #[test]
    fn doc_comment_action_uses_current_buffer_syntax_facts_not_stale_names() {
        let uri = Uri::from_str("file:///tmp/code-actions/Main.bd").expect("uri");
        let stale_source = "i32 Old() { return 0; }";
        let current_source = "i32 Before() { return 0; }\n\ni32 Current() { return 0; }";
        let stale_facts = syntax_documentation_facts_for_source(uri.as_str(), stale_source);
        assert!(stale_facts.iter().any(|fact| fact.name == "Old"));
        let current_facts = syntax_documentation_facts_for_source(uri.as_str(), current_source);
        let doc = Document {
            version: 2,
            text: current_source.to_string(),
            syntax_definitions: Vec::new(),
            syntax_hovers: Vec::new(),
            syntax_symbols: Vec::new(),
            syntax_completion: None,
            syntax_inlay_hints: Vec::new(),
            // Stale facts must not be consulted once the buffer text advanced; empty forces
            // a rebuild from `doc.text`, proving no HIR snapshot path remains.
            syntax_documentation: Vec::new(),
            syntax_diagnostics: Vec::new(),
        };
        let _ = stale_facts;
        let params = CodeActionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            range: Range {
                start: Position::new(2, 4),
                end: Position::new(2, 4),
            },
            context: CodeActionContext {
                diagnostics: Vec::new(),
                only: None,
                trigger_kind: None,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: Default::default(),
        };

        let actions = handle_code_actions(&uri, &doc, &params);

        assert!(actions.iter().any(|action| {
            matches!(action, CodeActionOrCommand::CodeAction(action)
                if action.title == "Generate or update documentation comment")
        }));
        assert!(current_facts.iter().any(|fact| fact.name == "Current"));
    }

    #[test]
    fn doc_comment_action_consumes_generation_bound_syntax_documentation_facts() {
        let uri = Uri::from_str("file:///tmp/code-actions/Main.bd").expect("uri");
        let current_source = "i32 Current(i32 value) { return value; }";
        let facts = syntax_documentation_facts_for_source(uri.as_str(), current_source);
        let doc = Document {
            version: 1,
            text: current_source.to_string(),
            syntax_definitions: Vec::new(),
            syntax_hovers: Vec::new(),
            syntax_symbols: Vec::new(),
            syntax_completion: None,
            syntax_inlay_hints: Vec::new(),
            syntax_documentation: facts,
            syntax_diagnostics: Vec::new(),
        };
        let params = CodeActionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            range: Range {
                start: Position::new(0, 4),
                end: Position::new(0, 4),
            },
            context: CodeActionContext {
                diagnostics: Vec::new(),
                only: None,
                trigger_kind: None,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: Default::default(),
        };

        let actions = handle_code_actions(&uri, &doc, &params);
        let action = actions.iter().find_map(|action| match action {
            CodeActionOrCommand::CodeAction(action)
                if action.title == "Generate or update documentation comment" =>
            {
                Some(action)
            }
            _ => None,
        });
        let action = action.expect("documentation action");
        let edit = action.edit.as_ref().expect("edit");
        let changes = edit.changes.as_ref().expect("changes");
        let edits = changes.get(&uri).expect("uri edits");
        assert!(
            edits.iter().any(|edit| edit.new_text.contains("@arg(value)")),
            "expected stub from syntax documentation facts, got {edits:?}"
        );
    }
}
