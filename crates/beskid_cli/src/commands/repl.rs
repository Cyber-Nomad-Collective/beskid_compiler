//! `beskid repl` — interactive snippet evaluator (JIT, no project resolve in v1).

use anyhow::Result;
use clap::Args;

#[derive(Args, Debug)]
pub struct ReplArgs {}

/// Run the snippet REPL on stdin until `:quit` or EOF.
pub fn execute(_args: ReplArgs) -> Result<()> {
    let mut session = beskid_repl::ReplSession::new();
    let mut input = beskid_repl::readline::StdioInput::new();
    beskid_repl::run(&mut session, &mut input).map_err(anyhow::Error::from)
}
