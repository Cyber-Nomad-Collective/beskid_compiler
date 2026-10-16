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

use crate::position::{offset_range_to_lsp, position_to_offset};
use crate::session::store::{Document, SyntaxTextEdit};

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
        Box::new(CompilerMoveProvider),
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

/// 0.6.2 ownership fixes: E1237 and E1242 insert `move ` before the named binding; E1239 on a
/// borrowed name and E1241 insert `move ` before the type of the parameter that declares it;
/// E1243 rewrites `move x.f` to `exchange(x.f, move replacement)`. Each edit is derived from the
/// parsed buffer and leaves source the formatter keeps unchanged.
struct CompilerMoveProvider;

const MOVE_CODES: &[&str] = &["E1237", "E1239", "E1241", "E1242", "E1243"];

impl CodeActionProvider for CompilerMoveProvider {
    fn handles(&self, source: &str, code: &str) -> bool {
        source == "beskid" && MOVE_CODES.contains(&code)
    }
    fn build(&self, uri: &Uri, doc: &Document, diag: &Diagnostic) -> Option<CodeAction> {
        let NumberOrString::String(code) = diag.code.as_ref()? else { return None };
        let start = position_to_offset(&doc.text, diag.range.start);
        let end = position_to_offset(&doc.text, diag.range.end);
        let site = doc.text.get(start..end)?;
        let (title, edit) = match code.as_str() {
            "E1237" | "E1242" => {
                let name_start = start + site.find(|c: char| c.is_ascii_alphabetic() || c == '_')?;
                ("Insert `move`".to_owned(), (name_start, name_start, "move ".to_owned()))
            }
            "E1239" | "E1241" => {
                let name = site.strip_prefix("move").map(str::trim_start).unwrap_or(site);
                let name = name.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).next()?;
                if name.is_empty() || name == "this" {
                    return None;
                }
                let type_start = parameter_type_start(&doc.text, start, name)?;
                ("Declare the parameter `move`".to_owned(), (type_start, type_start, "move ".to_owned()))
            }
            "E1243" => {
                let place = site.strip_prefix("move")?.trim_start();
                ("Rewrite to `exchange`".to_owned(), (start, end, format!("exchange({place}, move replacement)")))
            }
            _ => return None,
        };
        let mut changes = HashMap::new();
        changes.insert(
            uri.clone(),
            vec![TextEdit { range: offset_range_to_lsp(&doc.text, edit.0, edit.1), new_text: edit.2 }],
        );
        Some(CodeAction {
            title,
            kind: Some(CodeActionKind::QUICKFIX),
            diagnostics: Some(vec![diag.clone()]),
            edit: Some(WorkspaceEdit { changes: Some(changes), ..WorkspaceEdit::default() }),
            ..CodeAction::default()
        })
    }
}

/// The type offset of the parameter named `name` of the innermost function or method whose span
/// contains `offset`, when it is declared without `mut` or `move`.
fn parameter_type_start(text: &str, offset: usize, name: &str) -> Option<usize> {
    use beskid_analysis::syntax::{FunctionDefinition, MethodDefinition, Parameter, SyntaxGenerationId};
    use beskid_analysis::syntax_query::{NodeKind, SyntaxIndex};
    let program = beskid_analysis::services::parse_program(text).ok()?;
    let index = SyntaxIndex::from_program(&program, SyntaxGenerationId(0));
    let mut best: Option<(usize, usize)> = None;
    for kind in [NodeKind::FunctionDefinition, NodeKind::MethodDefinition] {
        for node in index.ids_of_kind(kind) {
            let Some(found) = index.node_at(&program, node) else { continue };
            let Some(span) = found.span() else { continue };
            if !(span.start <= offset && offset <= span.end) {
                continue;
            }
            let parameters: &[beskid_analysis::syntax::Spanned<Parameter>] =
                if let Some(function) = found.of::<FunctionDefinition>() {
                    &function.parameters
                } else if let Some(method) = found.of::<MethodDefinition>() {
                    &method.parameters
                } else {
                    continue;
                };
            let Some(parameter) = parameters.iter().find(|parameter| parameter.node.name.node.name == name) else {
                continue;
            };
            if parameter.node.mutable || parameter.node.movable {
                continue;
            }
            let width = span.end - span.start;
            if best.is_none_or(|(current, _)| width < current) {
                best = Some((width, parameter.node.ty.span.start));
            }
        }
    }
    best.map(|(_, start)| start)
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

fn syntax_text_edit_to_lsp(text: &str, edit: &SyntaxTextEdit) -> TextEdit {
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
    use crate::session::store::{Document, SyntaxFix, SyntaxTextEdit};
    use beskid_analysis::SyntaxTextEditKind;

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
            bsol_semantic_token_candidates: Vec::new(),
            syntax_attribute_names: Vec::new(),
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
        assert_eq!(action.kind, Some(CodeActionKind::QUICKFIX));
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
        // Partition is by `source`: a mod-source diagnostic is owned by the mod provider
        // regardless of code (build fail-closes if no matching SyntaxFix). Compiler codes
        // under a mod source are still routed to the mod provider, not the compiler ones.
        assert!(providers.iter().any(|p| p.handles("beskid:mod:ModA.Check", "W1503")));
        // Compiler providers never handle mod-source diagnostics (they require source == "beskid"),
        // and the mod provider never handles compiler-source diagnostics.
        assert!(!providers.iter().any(|p| p.handles("beskid", "MOD0001")));
    }

    fn apply_move_fix(text: &str, code: &str, start: usize, end: usize) -> Option<String> {
        let uri = Uri::from_str("file:///tmp/move-fix/Main.bd").expect("uri");
        let doc = doc_with_fixes(text, Vec::new());
        let range = crate::position::offset_range_to_lsp(text, start, end);
        let diag = Diagnostic {
            range,
            severity: Some(DiagnosticSeverity::ERROR),
            code: Some(NumberOrString::String(code.to_string())),
            source: Some("beskid".to_string()),
            message: "ownership".to_string(),
            ..Diagnostic::default()
        };
        let provider = code_action_providers().into_iter().find(|provider| provider.handles("beskid", code))?;
        let action = provider.build(&uri, &doc, &diag)?;
        let edit = action.edit?.changes?.remove(&uri)?.pop()?;
        let from = crate::position::position_to_offset(text, edit.range.start);
        let to = crate::position::position_to_offset(text, edit.range.end);
        Some(format!("{}{}{}", &text[..from], edit.new_text, &text[to..]))
    }

    #[test]
    fn ownership_quick_fixes_insert_move_and_rewrite_to_exchange() {
        let text = "[Isolated] pub type Archive { List<u8[]> blocks, pub Archive Add(move this, move u8[] block) { this.blocks = this.blocks.Push(block); return this; } }";
        let at = text.find("Push(block)").unwrap() + 5;
        let fixed = apply_move_fix(text, "E1242", at, at + 5).expect("E1242 fix");
        assert!(fixed.contains("this.blocks.Push(move block)"), "{fixed}");

        let text = "unit Keep(u8[] data, Archive a) { a.Add(move data); }";
        let at = text.find("move data").unwrap();
        let fixed = apply_move_fix(text, "E1239", at, at + 9).expect("E1239 fix");
        assert!(fixed.starts_with("unit Keep(move u8[] data, Archive a)"), "{fixed}");

        let text = "[Isolated] pub type Pair { Index left, pub Index TakeLeft(move this) { return move this.left; } }";
        let at = text.find("move this.left").unwrap();
        let fixed = apply_move_fix(text, "E1243", at, at + 14).expect("E1243 fix");
        assert!(fixed.contains("return exchange(this.left, move replacement);"), "{fixed}");
    }
}

