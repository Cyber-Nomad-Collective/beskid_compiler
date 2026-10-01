# beskid_tools

Shared command infrastructure for the `beskid` CLI: line-oriented pipeline
progress, diagnostic rendering, Corelib provisioning, registry helpers, prompts,
and command sessions.

Domain crates (`beskid_repl`, `beskid_template`, `beskid_lsp`, and others)
own feature behavior. This crate owns the plumbing used by the CLI wrappers.

## Pipeline output

`CliPipeline` observes compiler phases and work units. Phase trees and command
summaries remain in terminal scrollback. Interactive stderr may display one
bounded progress bar; redirected output, CI, and `--plain` use ordinary lines.
Pipeline output does not read terminal input or wait after command completion.

`pipeline::tui` contains the line-output value and formatting helpers used by
existing command wrappers. Its summaries and test-result presenter have no
full-screen runtime or widget dependencies.

## Prompts and sessions

`prompt` provides line-oriented confirmations shared by template and registry
commands. `CommandSession` connects input resolution, semantic gates, and
pipeline observation. `run_on_compiler_stack` gives compilation a suitable
worker-thread stack on each platform.

Graph rendering belongs to the CLI's independent `graphs-tui` integration.
