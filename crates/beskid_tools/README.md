# beskid_tools

Shared command infrastructure for the `beskid` CLI and future tooling binaries.

## Role

`beskid_cli` stays a thin Clap binary. Cross-cutting plumbing lives here:

- **pipeline** — `CliPipeline`, resolve helpers, progress TUI
- **session** — `CommandSession` for resolve + semantic gate flows
- **diagnostics** — miette formatting without indicatif interleaving
- **corelib** — bundled corelib provisioning (`ensure_bundled_corelib`)
- **registry** — pckg client helpers (`RegistryConnectConfig`, version pick)
- **prompt** — stdin confirm helpers for template commands
- **toolchain** — GitHub release install for managed LSP binaries
- **logging** — `env_logger` defaults

Domain behavior belongs in dedicated crates (`beskid_template`, `beskid_repl`, `beskid_lsp`, `beskid_pckg`, …).

## Adding a new command

1. Implement behavior in the appropriate domain crate (or extend `beskid_tools` only for shared plumbing).
2. Add `compiler/crates/beskid_cli/src/commands/<name>.rs` with Clap `*Args` and `execute()`.
3. Register the subcommand in `cli.rs`.
4. For project-scoped compile commands, prefer `CommandSession::open_and_resolve` + `semantic_gate`.
5. Update platform spec under `site/website/src/content/docs/platform-spec/tooling/cli/` before observable behavior changes.
