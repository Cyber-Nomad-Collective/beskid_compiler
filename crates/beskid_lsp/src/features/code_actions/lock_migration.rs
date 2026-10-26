//! "Migrate lock file" quick fix for a `Project.lock` in a retired format (v1 or v2).
//!
//! The diagnostic comes from the lock replay check (`ProjectError::LockfileFormatRetired`, code `E3024`).
//! The fix runs the client command `beskid.migrateLock` with the URI of the lock file, which the
//! extension turns into `beskid dev project lock`. The server writes nothing itself.

use std::path::Path;

use tower_lsp_server::ls_types::{CodeAction, CodeActionKind, Command, Diagnostic, Uri};

use crate::session::store::Document;

use super::registry::CodeActionProvider;

/// Diagnostic code of `ProjectError::LockfileFormatRetired`.
pub(super) const RETIRED_LOCK_CODE: &str = "E3024";
/// Client command registered by the VS Code extension.
pub(super) const MIGRATE_LOCK_COMMAND: &str = "beskid.migrateLock";

pub(super) struct LockMigrationProvider;

impl CodeActionProvider for LockMigrationProvider {
    fn handles(&self, source: &str, code: &str) -> bool {
        source == "beskid" && code == RETIRED_LOCK_CODE
    }

    fn build(&self, uri: &Uri, _doc: &Document, diag: &Diagnostic) -> Option<CodeAction> {
        let lock = nearest_project_lock(&uri.to_file_path()?)?;
        let lock_uri = crate::workspace_scan::path_to_uri_string(&lock);
        Some(CodeAction {
            title: "Migrate lock file".to_string(),
            kind: Some(CodeActionKind::QUICKFIX),
            diagnostics: Some(vec![diag.clone()]),
            command: Some(Command {
                title: "Migrate lock file".to_string(),
                command: MIGRATE_LOCK_COMMAND.to_string(),
                arguments: Some(vec![serde_json::Value::String(lock_uri)]),
            }),
            ..CodeAction::default()
        })
    }
}

/// The `Project.lock` of the project that owns `file`: the first one found walking up.
fn nearest_project_lock(file: &Path) -> Option<std::path::PathBuf> {
    file.ancestors().skip(1).map(|dir| dir.join("Project.lock")).find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use tower_lsp_server::ls_types::{
        CodeActionContext, CodeActionOrCommand, CodeActionParams, DiagnosticSeverity, NumberOrString, Position, Range,
        TextDocumentIdentifier, WorkDoneProgressParams,
    };

    use super::*;
    use crate::features::code_actions::registry::code_action_providers;

    fn lock_diag() -> Diagnostic {
        Diagnostic {
            range: Range { start: Position::new(0, 0), end: Position::new(0, 1) },
            severity: Some(DiagnosticSeverity::ERROR),
            code: Some(NumberOrString::String(RETIRED_LOCK_CODE.to_string())),
            source: Some("beskid".to_string()),
            message: "Project.lock uses the retired v1 format".to_string(),
            ..Diagnostic::default()
        }
    }

    fn document(text: &str) -> Document {
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
            syntax_fixes: Vec::new(),
        }
    }

    #[test]
    fn registry_routes_the_v1_lock_code_to_the_migration_provider() {
        let providers = code_action_providers();
        assert!(providers.iter().any(|p| p.handles("beskid", RETIRED_LOCK_CODE)));
    }

    #[test]
    fn v1_lock_diagnostic_offers_a_migrate_command_with_the_lock_uri() {
        let temp = tempfile::TempDir::new().expect("temp project");
        std::fs::create_dir_all(temp.path().join("Src")).expect("src");
        std::fs::write(temp.path().join("Project.lock"), "# Project.lock v1\n").expect("lock");
        let source = temp.path().join("Src/Main.bd");
        std::fs::write(&source, "i32 Main() { return 0; }\n").expect("source");
        let uri = Uri::from_str(&crate::workspace_scan::path_to_uri_string(&source)).expect("uri");
        let doc = document("i32 Main() { return 0; }\n");
        let params = CodeActionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            range: Range { start: Position::new(0, 0), end: Position::new(0, 0) },
            context: CodeActionContext { diagnostics: vec![lock_diag()], only: None, trigger_kind: None },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: Default::default(),
        };

        let actions = crate::features::code_actions::handler::handle_code_actions(&uri, &doc, &params);
        let action = actions
            .iter()
            .find_map(|a| match a {
                CodeActionOrCommand::CodeAction(a) if a.title == "Migrate lock file" => Some(a),
                _ => None,
            })
            .expect("migrate lock quick fix");
        assert_eq!(action.kind, Some(CodeActionKind::QUICKFIX));
        let command = action.command.as_ref().expect("client command");
        assert_eq!(command.command, "beskid.migrateLock");
        let argument = command.arguments.as_ref().and_then(|args| args.first()).and_then(|v| v.as_str()).expect("uri");
        assert!(argument.starts_with("file:"), "{argument}");
        assert!(argument.ends_with("/Project.lock"), "{argument}");
    }

    #[test]
    fn v1_lock_diagnostic_on_the_manifest_offers_the_migrate_command() {
        let temp = tempfile::TempDir::new().expect("temp project");
        std::fs::write(temp.path().join("Project.lock"), "# Project.lock v1\n").expect("lock");
        let manifest = temp.path().join("App.bproj");
        std::fs::write(&manifest, "").expect("manifest");
        let uri = Uri::from_str(&crate::workspace_scan::path_to_uri_string(&manifest)).expect("uri");
        let params = CodeActionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            range: Range { start: Position::new(0, 0), end: Position::new(0, 0) },
            context: CodeActionContext { diagnostics: vec![lock_diag()], only: None, trigger_kind: None },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: Default::default(),
        };
        let actions = crate::features::code_actions::handler::handle_code_actions(&uri, &document(""), &params);
        assert!(
            actions.iter().any(|a| matches!(a,
                CodeActionOrCommand::CodeAction(a) if a.command.as_ref().is_some_and(|c| c.command == "beskid.migrateLock"))),
            "manifest must offer the migrate fix: {actions:?}"
        );
    }

    #[test]
    fn without_a_lock_file_no_action_is_offered() {
        let temp = tempfile::TempDir::new().expect("temp project");
        let source = temp.path().join("Main.bd");
        std::fs::write(&source, "").expect("source");
        let uri = Uri::from_str(&crate::workspace_scan::path_to_uri_string(&source)).expect("uri");
        assert!(LockMigrationProvider.build(&uri, &document(""), &lock_diag()).is_none());
    }
}
