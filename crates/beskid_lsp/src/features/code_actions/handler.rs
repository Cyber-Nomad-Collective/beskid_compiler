use std::collections::HashMap;

use beskid_analysis::doc::DocCommentEdit;
use tower_lsp_server::ls_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, CodeActionResponse, Diagnostic, NumberOrString,
    TextEdit, Uri, WorkspaceEdit,
};

use crate::features::formatting;
use crate::features::project_manifest::api as project_manifest;
use crate::position::{offset_range_to_lsp, position_to_offset};
use crate::session::documentation_facts::{doc_comment_edit_from_syntax_facts, syntax_documentation_facts_for_source};
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
        DocCommentEdit::Replace { start, end, text } => (offset_range_to_lsp(&doc.text, start, end), text),
    };
    let mut changes = HashMap::new();
    changes.insert(uri.clone(), vec![TextEdit { range, new_text }]);
    Some(CodeAction {
        title: title.to_string(),
        kind: Some(if diagnostics.is_some() { CodeActionKind::QUICKFIX } else { CodeActionKind::SOURCE }),
        diagnostics,
        edit: Some(WorkspaceEdit { changes: Some(changes), ..WorkspaceEdit::default() }),
        ..CodeAction::default()
    })
}

pub(super) fn remove_lines_action(uri: &Uri, doc: &Document, diag: &Diagnostic) -> Option<CodeAction> {
    let start = position_to_offset(&doc.text, diag.range.start);
    let end = position_to_offset(&doc.text, diag.range.end);
    let (line_start, line_end) = line_span(&doc.text, start, end);
    let range = offset_range_to_lsp(&doc.text, line_start, line_end);
    let mut changes = HashMap::new();
    changes.insert(uri.clone(), vec![TextEdit { range, new_text: String::new() }]);
    Some(CodeAction {
        title: "Remove unused import".to_string(),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diag.clone()]),
        edit: Some(WorkspaceEdit { changes: Some(changes), ..WorkspaceEdit::default() }),
        ..CodeAction::default()
    })
}

pub(super) fn remove_range_action(uri: &Uri, diag: &Diagnostic, title: &'static str) -> Option<CodeAction> {
    if diag.range.start == diag.range.end {
        return None;
    }

    let mut changes = HashMap::new();
    changes.insert(uri.clone(), vec![TextEdit { range: diag.range, new_text: String::new() }]);
    Some(CodeAction {
        title: title.to_string(),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diag.clone()]),
        edit: Some(WorkspaceEdit { changes: Some(changes), ..WorkspaceEdit::default() }),
        ..CodeAction::default()
    })
}

pub(super) fn line_span(source: &str, start_off: usize, end_off: usize) -> (usize, usize) {
    let start_off = start_off.min(source.len());
    let end_off = end_off.min(source.len()).max(start_off);
    let line_start = source[..start_off].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = if let Some(rel) = source[end_off..].find('\n') { end_off + rel + 1 } else { source.len() };
    (line_start, line_end)
}

pub(super) fn doc_comment_quickfix(
    uri: &Uri,
    doc: &Document,
    diag: &Diagnostic,
    title: &'static str,
) -> Option<CodeAction> {
    doc_comment_code_action(uri, doc, position_to_offset(&doc.text, diag.range.start), title, Some(vec![diag.clone()]))
}

/// Quick fixes (e.g. doc comments), manifest assists, and source actions such as format-document.
///
/// Diagnostic-driven quick-fixes are routed through the [`CodeActionProvider`] registry
/// (see `registry.rs`). Each provider owns a class of diagnostics keyed by `(source, code)`:
/// compiler-origin fixes (`source == "beskid"`) are handled by the `Compiler*Provider`s;
/// mod-origin fixes (`source.starts_with("beskid:mod:")`) are handled by
/// `ModQuickFixProvider`, which reads generation-bound `Document.syntax_fixes`.
pub fn handle_code_actions(uri: &Uri, doc: &Document, params: &CodeActionParams) -> CodeActionResponse {
    let mut actions = Vec::new();
    code_actions_into(uri, doc, params, &mut |action| {
        actions.push(action);
        true
    });
    CodeActionResponse::from(actions)
}

/// Compute the actions of [`handle_code_actions`] one by one, cheapest first: the fixes the
/// compiler attached to a diagnostic, the providers' quick fixes, then the source actions
/// (whole-document formatting, the documentation comment). Each action goes to `emit` as soon as
/// it is computed; when `emit` returns `false` the caller no longer waits, and no further action
/// is computed.
pub fn code_actions_into(
    uri: &Uri,
    doc: &Document,
    params: &CodeActionParams,
    emit: &mut dyn FnMut(CodeActionOrCommand) -> bool,
) {
    let started = std::time::Instant::now();
    let mut phases: Vec<(&str, u128)> = Vec::new();
    let finished = code_action_phases(uri, doc, params, emit, &mut |phase, at| {
        phases.push((phase, at.elapsed().as_millis()));
        *at = std::time::Instant::now();
    });
    crate::trace::event("code-action-compute", || {
        let phases: Vec<String> = phases.iter().map(|(phase, ms)| format!("{phase}={ms}")).collect();
        format!(
            "uri={} len={} doc_facts_stored={} finished={finished} total_ms={} {}",
            uri.as_str(),
            doc.text.len(),
            !doc.syntax_documentation.is_empty(),
            started.elapsed().as_millis(),
            phases.join(" ")
        )
    });
}

/// `false` when `emit` stopped the computation.
fn code_action_phases(
    uri: &Uri,
    doc: &Document,
    params: &CodeActionParams,
    emit: &mut dyn FnMut(CodeActionOrCommand) -> bool,
    mark: &mut dyn FnMut(&'static str, &mut std::time::Instant),
) -> bool {
    let mut at = std::time::Instant::now();
    // Fixes the compiler attached to a diagnostic travel in its `data`, for source files and
    // project manifests alike.
    for diag in &params.context.diagnostics {
        for action in crate::diagnostics::quick_fixes_from_data(uri, &doc.text, diag) {
            if !emit(CodeActionOrCommand::CodeAction(action)) {
                return false;
            }
        }
    }
    mark("quick_ms", &mut at);

    // Diagnostic-driven providers run for manifests too: project diagnostics are published on
    // the `.bproj`, and that is where the editor asks for their fixes.
    let providers = super::registry::code_action_providers();
    for diag in &params.context.diagnostics {
        let source = diag.source.as_deref().unwrap_or("beskid");
        let Some(NumberOrString::String(code)) = &diag.code else { continue };
        for provider in &providers {
            if provider.handles(source, code)
                && let Some(action) = provider.build(uri, doc, diag)
                && !emit(CodeActionOrCommand::CodeAction(action))
            {
                return false;
            }
        }
    }
    mark("providers_ms", &mut at);

    if project_manifest::is_manifest_uri(uri) {
        return true;
    }
    if let Some(edits) =
        formatting::handler::handle_document_formatting(&doc.text).ok().filter(|edits| !edits.is_empty())
    {
        let mut changes = HashMap::new();
        changes.insert(uri.clone(), edits);
        let format = CodeAction {
            title: "Format document".to_string(),
            kind: Some(CodeActionKind::SOURCE),
            edit: Some(WorkspaceEdit { changes: Some(changes), ..WorkspaceEdit::default() }),
            ..CodeAction::default()
        };
        if !emit(CodeActionOrCommand::CodeAction(format)) {
            return false;
        }
    }
    mark("format_ms", &mut at);

    if let Some(path) = uri.to_file_path()
        && path.extension().and_then(|e| e.to_str()) == Some("bd")
    {
        let offset = position_to_offset(&doc.text, params.range.start);
        if let Some(action) =
            doc_comment_code_action(uri, doc, offset, "Generate or update documentation comment", None)
            && !emit(CodeActionOrCommand::CodeAction(action))
        {
            return false;
        }
    }
    mark("doc_ms", &mut at);
    true
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use tower_lsp_server::ls_types::{
        CodeActionContext, CodeActionOrCommand, CodeActionParams, Position, Range, TextDocumentIdentifier, Uri,
        WorkDoneProgressParams,
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
            bsol_semantic_token_candidates: Vec::new(),
            syntax_attribute_names: Vec::new(),
            syntax_completion: None,
            syntax_inlay_hints: Vec::new(),
            // Stale facts must not be consulted once the buffer text advanced; empty forces
            // a rebuild from `doc.text`, proving no HIR snapshot path remains.
            syntax_documentation: Vec::new(),
            syntax_diagnostics: Vec::new(),
            syntax_fixes: Vec::new(),
        };
        let _ = stale_facts;
        let params = CodeActionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            range: Range { start: Position::new(2, 4), end: Position::new(2, 4) },
            context: CodeActionContext { diagnostics: Vec::new(), only: None, trigger_kind: None },
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
            bsol_semantic_token_candidates: Vec::new(),
            syntax_attribute_names: Vec::new(),
            syntax_completion: None,
            syntax_inlay_hints: Vec::new(),
            syntax_documentation: facts,
            syntax_diagnostics: Vec::new(),
            syntax_fixes: Vec::new(),
        };
        let params = CodeActionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            range: Range { start: Position::new(0, 4), end: Position::new(0, 4) },
            context: CodeActionContext { diagnostics: Vec::new(), only: None, trigger_kind: None },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: Default::default(),
        };

        let actions = handle_code_actions(&uri, &doc, &params);
        let action = actions.iter().find_map(|action| match action {
            CodeActionOrCommand::CodeAction(action) if action.title == "Generate or update documentation comment" => {
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

    #[test]
    fn manifest_diagnostics_offer_their_compiler_fixes_as_quick_fixes() {
        use crate::diagnostics::{DiagnosticDetail, FixFact, lsp_diagnostics_from_syntax};
        use crate::session::store::{SyntaxDiagnostic, SyntaxDiagnosticSeverity};
        let uri = Uri::from_str("file:///tmp/code-actions/App.bproj").expect("uri");
        let source = "Ap {\n  name = \"App\"\n}\n";
        let fact = SyntaxDiagnostic {
            start: 0,
            end: 2,
            severity: SyntaxDiagnosticSeverity::Error,
            code: Some("E1896".into()),
            message: "The project block is named `Ap`.".into(),
            source: "beskid".into(),
            detail: DiagnosticDetail {
                uri: Some(uri.to_string()),
                fixes: vec![FixFact {
                    title: "Rename the block to `App`".into(),
                    applicability: beskid_analysis::Applicability::MachineApplicable,
                    edits: vec![(0, 2, "App".into())],
                }],
                ..DiagnosticDetail::default()
            },
        };
        let diagnostics = lsp_diagnostics_from_syntax(source, &[fact]);
        let doc = Document {
            version: 1,
            text: source.to_string(),
            syntax_definitions: Vec::new(),
            syntax_hovers: Vec::new(),
            syntax_symbols: Vec::new(),
            bsol_semantic_token_candidates: Vec::new(),
            syntax_attribute_names: Vec::new(),
            syntax_completion: None,
            syntax_inlay_hints: Vec::new(),
            syntax_documentation: Vec::new(),
            syntax_diagnostics: Vec::new(),
            syntax_fixes: Vec::new(),
        };
        let params = CodeActionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            range: Range { start: Position::new(0, 0), end: Position::new(0, 2) },
            context: CodeActionContext { diagnostics, only: None, trigger_kind: None },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: Default::default(),
        };

        let actions = handle_code_actions(&uri, &doc, &params);
        let fix = actions.iter().find_map(|action| match action {
            CodeActionOrCommand::CodeAction(action) if action.title == "Rename the block to `App`" => Some(action),
            _ => None,
        });
        let fix = fix.expect("manifest quick fix");
        assert_eq!(fix.is_preferred, Some(true));
        assert_eq!(fix.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri][0].new_text, "App");
    }
}
