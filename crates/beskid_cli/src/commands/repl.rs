//! `beskid repl` — interactive snippet evaluator (JIT, no project resolve in v1).

use anyhow::Result;
use clap::Args;
use std::io::IsTerminal;

use crate::runtime_profile::CliRuntimeProfile;

#[derive(Args, Debug)]
pub struct ReplArgs {
    /// Disable Ratatui REPL; use line-oriented stdin/stdout instead.
    #[arg(long)]
    pub plain: bool,

    /// Runtime link profile: `std` links `beskid_host`; `minimal` is language runtime only
    #[arg(long, value_enum, default_value_t = CliRuntimeProfile::Std)]
    pub runtime_profile: CliRuntimeProfile,
}

/// Run the snippet REPL on stdin until `:quit` or EOF.
pub fn execute(args: ReplArgs) -> Result<()> {
    let mut session = beskid_repl::ReplSession::with_link_profile(args.runtime_profile.into());
    if args.plain || !std::io::stdin().is_terminal() {
        let mut input = beskid_repl::readline::StdioInput::new();
        beskid_repl::run(&mut session, &mut input).map_err(anyhow::Error::from)
    } else {
        beskid_repl::run_tui(&mut session).map_err(anyhow::Error::from)
    }
}
