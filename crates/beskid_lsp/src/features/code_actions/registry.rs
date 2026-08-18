//! Code-action provider registry.
//!
//! Diagnostic-driven quick-fixes are routed through [`CodeActionProvider`] impls keyed by
//! `(source, code)`. Compiler-origin fixes (`source == "beskid"`) reuse the existing
//! handlers in `super::handler`; mod-origin fixes (`source.starts_with("beskid:mod:")`)
//! read generation-bound [`crate::session::store::SyntaxFix`] facts from the `Document`.
//!
//! The registry is an internal implementation detail — the LSP `codeAction` capability
//! shape (`Simple(true)` in `server/init.rs`) is unchanged.

use std::collections::HashMap;

use tower_lsp_server::ls_types::{
    CodeAction, CodeActionKind, Diagnostic, NumberOrString, TextEdit, Uri, WorkspaceEdit,
};

use crate::position::offset_range_to_lsp;
use crate::session::store::{Document, SyntaxFix, SyntaxTextEditKind};

use super::handler::{doc_comment_quickfix, remove_lines_action, remove_range_action};

/// A code-action provider owns fixes for a class of diagnostics keyed by `(source, code)`.
pub(super) trait CodeActionProvider: Send + Sync {
    /// Does this provider own fixes for the given `(source, code)`?
    fn handles(&self, source: &str, code: &str) -> bool;
    /// Build the LSP [`CodeAction`] for the diagnostic, reading generation-bound facts from `doc`.
    fn build(&self, uri: &Uri, doc: &Document, diag: &Diagnostic) -> Option<CodeAction>;
}

/// Construct the provider registry. Order matters only for diagnostics that match more than
/// one provider; in practice `(source, code)` partitions the space (compiler codes are
/// `source == "beskid"`, mod codes are `source == "beskid:mod:..."`).
pub(super) fn code_action_providers() -> Vec<Box<dyn CodeActionProvider>> {
    vec![
        Box::new(CompilerRemoveLinesProvider),
        Box::new(CompilerRemoveRangeProvider),
        Box::new(CompilerDocCommentProvider),
        Box::new(ModQuickFixProvider),
    ]
}

/// `W1503` — remove unused import lines.
struct CompilerRemoveLinesProvider;

impl CodeActionProvider for CompilerRemoveLinesProvider {
    fn handles(&self, source: &str, code: &str) -> bool {
        source == "beskid" && code == "W1503"
    }
    fn build(&self, uri: &Uri, doc: &Document, diag: &Diagnostic) -> Option<CodeAction> {
        remove_lines_action(uri, doc, diag)
    }
}

/// `W1639` — remove empty enum constructor parens.
struct CompilerRemoveRangeProvider;

impl CodeActionProvider for CompilerRemoveRangeProvider {
    fn handles(&self, source: &str, code: &str) -> bool {
        source == "beskid" && code == "W1639"
    }
    fn build(&self, uri: &Uri, _doc: &Document, diag: &Diagnostic) -> Option<CodeAction> {
        remove_range_action(uri, diag, "Remove empty enum constructor parens")
    }
}

/// `W1610`–`W1625` — update documentation comment.
struct CompilerDocCommentProvider;

const DOC_COMMENT_CODES: &[&str] =
    &["W1610", "W1611", "W1612", "W1613", "W1614", "W1615", "W1620", "W1621", "W1622", "W1623", "W1624", "W1625"];

impl CodeActionProvider for CompilerDocCommentProvider {
    fn handles(&self, source: &str, code: &str) -> bool {
        source == "beskid" && DOC_COMMENT_CODES.contains(&code)
    }
    fn build(&self, uri: &Uri, doc: &Document, diag: &Diagnostic) -> Option<CodeAction> {
        doc_comment_quickfix(uri, doc, diag, "Update documentation comment")
    }
}

/// Mod-origin quick-fixes. Routes any diagnostic whose `source` starts with
/// `beskid:mod:` to the generation-bound [`SyntaxFix`] matching `(source, code)` on
/// `Document.syntax_fixes`. Converts [`SyntaxTextEdit`] → LSP [`TextEdit`] (byte offsets
/// → positions) and returns a `QUICKFIX` action carrying the diagnostic and workspace edit.
struct ModQuickFixProvider;

impl CodeActionProvider for ModQuickFixProvider {
    fn handles(&self, source: &str, _code: &str) -> bool {
        source.starts_with("beskid:mod:")
    }
    fn build(&self, uri: &Uri, doc: &Document, diag: &Diagnostic) -> Option<CodeAction> {
        let source = diag.source.as_deref()?;
        let NumberOrString::String(code) = diag.code.as_ref()? else { return None };
        // Find the generation-bound fix matching (source, code). First-match is acceptable
        // for Phase 1 (mod fix ordering follows mod.load discovery order, which is not
        // deterministic across workspace reshuffles — documented in the design).
        let fix = doc.syntax_fixes.iter().find(|f| f.source == source && f.diagnostic_code == *code)?;
        let lsp_edits: Vec<TextEdit> = fix.edits.iter().map(|edit| syntax_text_edit_to_lsp(&doc.text, edit)).collect();
        let mut changes = HashMap::new();
        changes.insert(uri.clone(), lsp_edits);
        Some(CodeAction {
            title: fix.title.clone(),
            kind: Some(CodeActionKind::QUICKFIX),
            diagnostics: Some(vec![diag.clone()]),
            edit: Some(WorkspaceEdit { changes: Some(changes), ..WorkspaceEdit::default() }),
            ..CodeAction::default()
        })
    }
}

fn syntax_text_edit_to_lsp(text: &str, edit: &beskid_analysis::SyntaxTextEdit) -> TextEdit {
    // Insert edits carry `start == end`; `offset_range_to_lsp` produces a zero-width range
    // at the insertion point. Delete/Replace carry a non-empty range.
    let range = offset_range_to_lsp(text, edit.start, edit.end);
    TextEdit { range, new_text: edit.text.clone() }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use tower_lsp_server::ls_types::{
        CodeActionContext, CodeActionKind, CodeActionOrCommand, CodeActionParams, Diagnostic, DiagnosticSeverity,
        NumberOrString, Position, Range, TextDocumentIdentifier, Uri, WorkDoneProgressParams,
    };

    use super::code_action_providers;
    use crate::session::store::{Document, SyntaxFix, SyntaxTextEdit, SyntaxTextEditKind};

    fn mod_diag(source: &str, code: &str, start: Position, end: Position) -> Diagnostic {
        Diagnostic {
            range: Range { start, end },
            severity: Some(DiagnosticSeverity::WARNING),
            code: Some(NumberOrString::String(code.to_string())),
            source: Some(source.to_string()),
            message: "mod issue".to_string(),
            ..Diagnostic::default()
        }
    }

    fn doc_with_fixes(text: &str, fixes: Vec<SyntaxFix>) -> Document {
        Document {
            version: 1,
            text: text.to_string(),
            syntax_definitions: Vec::new(),
            syntax_hovers: Vec::new(),
            syntax_symbols: Vec::new(),
            syntax_completion: None,
            syntax_inlay_hints: Vec::new(),
            syntax_documentation: Vec::new(),
            syntax_diagnostics: Vec::new(),
            syntax_fixes: fixes,
        }
    }

    /// A mod-origin diagnostic yields a `QUICKFIX` action from `ModQuickFixProvider` reading
    /// `Document.syntax_fixes`, carrying the diagnostic and a workspace edit built from the
    /// fix's `SyntaxTextEdit`s.
    #[test]
    fn mod_origin_diagnostic_yields_quickfix_from_syntax_fixes() {
        let uri = Uri::from_str("file:///tmp/mod-fix/Main.bd").expect("uri");
        // "unit Main() { return; }" — replace bytes 5..9 ("Main") with "Entry".
        let text = "unit Main() { return; }";
        let fixes = vec![SyntaxFix {
            source: "beskid:mod:ModA.Check".to_string(),
            diagnostic_code: "MOD0001".to_string(),
            title: "Rename to Entry".to_string(),
            edits: vec![SyntaxTextEdit {
                kind: SyntaxTextEditKind::Replace,
                start: 5,
                end: 9,
                text: "Entry".to_string(),
            }],
        }];
        let doc = doc_with_fixes(text, fixes);
        let diag = mod_diag("beskid:mod:ModA.Check", "MOD0001", Position::new(0, 5), Position::new(0, 9));
        let params = CodeActionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            range: Range { start: Position::new(0, 5), end: Position::new(0, 9) },
            context: CodeActionContext { diagnostics: vec![diag.clone()], only: None, trigger_kind: None },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: Default::default(),
        };

        let actions = super::super::handler::handle_code_actions(&uri, &doc, &params);
        let action = actions
            .iter()
            .find_map(|a| match a {
                CodeActionOrCommand::CodeAction(a) if a.title == "Rename to Entry" => Some(a),
                _ => None,
            })
            .expect("mod QUICKFIX action");
        assert_eq!(action.kind.as_deref(), Some(CodeActionKind::QUICKFIX));
        let edit = action.edit.as_ref().expect("edit");
        let changes = edit.changes.as_ref().expect("changes");
        let edits = changes.get(&uri).expect("uri edits");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, "Entry");
        // The action carries the diagnostic it fixes.
        let linked = action.diagnostics.as_ref().expect("diagnostics");
        assert_eq!(linked.len(), 1);
        assert_eq!(linked[0].code.as_ref(), Some(&NumberOrString::String("MOD0001".to_string())));
    }

    /// A mod-origin diagnostic with no matching `SyntaxFix` on the `Document` yields no action
    /// (fail-closed — the provider does not invent a fix).
    #[test]
    fn mod_origin_diagnostic_without_matching_fix_yields_no_action() {
        let uri = Uri::from_str("file:///tmp/mod-fix-empty/Main.bd").expect("uri");
        let text = "unit Main() { return; }";
        let doc = doc_with_fixes(text, Vec::new());
        let diag = mod_diag("beskid:mod:ModA.Check", "MOD0001", Position::new(0, 5), Position::new(0, 9));
        let params = CodeActionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            range: Range { start: Position::new(0, 5), end: Position::new(0, 9) },
            context: CodeActionContext { diagnostics: vec![diag], only: None, trigger_kind: None },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: Default::default(),
        };

        let actions = super::super::handler::handle_code_actions(&uri, &doc, &params);
        assert!(
            !actions.iter().any(|a| matches!(a, CodeActionOrCommand::CodeAction(a) if a.title == "Rename to Entry")),
            "no matching fix → no action"
        );
    }

    /// The registry routes compiler-origin `W1503` to `CompilerRemoveLinesProvider` and
    /// mod-origin codes to `ModQuickFixProvider` (partitions by `source`).
    #[test]
    fn registry_partitions_by_source() {
        let providers = code_action_providers();
        // Compiler W1503 → CompilerRemoveLinesProvider (source == "beskid").
        assert!(providers.iter().any(|p| p.handles("beskid", "W1503")));
        // Mod-origin code → ModQuickFixProvider (source starts with "beskid:mod:").
        assert!(providers.iter().any(|p| p.handles("beskid:mod:ModA.Check", "MOD0001")));
        // Compiler code is NOT handled by the mod provider.
        assert!(!providers.iter().any(|p| p.handles("beskid:mod:ModA.Check", "W1503")));
    }
}
