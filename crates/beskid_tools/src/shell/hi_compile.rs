//! In-process `build` / `test` from `beskid hi` (registered by `beskid_cli`).

use std::sync::mpsc::Sender;

use anyhow::Result;

use super::scope::ShellScope;
use crate::tui::shell::runtime::RuntimeOp;

/// Queued in-process compile command (handled on a background thread).
#[derive(Debug, Clone)]
pub struct HiCompileJob {
    pub command: String,
    pub params: String,
}

/// Inputs for a compile command launched inside the hi shell event loop.
pub struct HiCompileRequest<'a> {
    pub command: &'a str,
    pub params: &'a str,
    pub scope: &'a ShellScope,
    pub msg_tx: Sender<RuntimeOp>,
}

/// Runs `build` or `test` in-process with pipeline progress forwarded to the hi shell.
pub type HiCompileRegistrar = fn(HiCompileRequest<'_>) -> Result<()>;

const IN_PROCESS_COMMANDS: &[&str] = &["build", "test"];

pub fn is_in_process_command(command: &str) -> bool {
    IN_PROCESS_COMMANDS.contains(&command)
}
