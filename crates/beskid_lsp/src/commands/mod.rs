//! `workspace/executeCommand` routing for Beskid extension commands.

pub mod pckg_registry;
pub mod project_explorer;

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::RwLock;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::LSPAny;

pub use pckg_registry::PckgRegistryState;
pub use project_explorer::{
    PROJECT_EXPLORER_COMMANDS, focused_project_from_configuration, focused_project_from_value,
    handle_project_explorer_command,
};

/// Dispatch a workspace execute command to pckg or project-explorer handlers.
pub async fn handle_execute_command(
    command: &str,
    arguments: Option<Vec<Value>>,
    workspace_roots: &[PathBuf],
    pckg_registry: &Arc<RwLock<PckgRegistryState>>,
) -> Result<Option<LSPAny>> {
    if let Some(result) = pckg_registry::handle_pckg_registry_command(
        command,
        arguments.clone(),
        workspace_roots,
        pckg_registry,
    )
    .await?
    {
        return Ok(Some(result));
    }
    handle_project_explorer_command(command, arguments, workspace_roots)
}
