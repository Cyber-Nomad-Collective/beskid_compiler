//! `beskid repl` — interactive snippet evaluator (JIT, no project resolve in v1).

use anyhow::Result;
use clap::Args;
use std::io::IsTerminal;

#[derive(Args, Debug)]
pub struct ReplArgs {
    /// Disable Ratatui REPL; use line-oriented stdin/stdout instead.
    #[arg(long)]
    pub plain: bool,
}

/// Run the snippet REPL on stdin until `:quit` or EOF.
pub fn execute(args: ReplArgs) -> Result<()> {
    let mut session = beskid_repl::ReplSession::new();
    if args.plain || !std::io::stdin().is_terminal() {
        let mut input = beskid_repl::readline::StdioInput::new();
        beskid_repl::run(&mut session, &mut input)
    } else {
        beskid_repl::run_tui(&mut session)
    }
}
