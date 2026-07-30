//! Shared command infrastructure for the `beskid` CLI and future tooling binaries.
//!
//! Domain crates (`beskid_repl`, `beskid_template`, `beskid_lsp`, …) own feature behavior;
//! this crate owns cross-cutting plumbing: pipeline progress UI, diagnostic rendering, corelib
//! provisioning, registry client helpers, and session facades for resolve + semantic gates.
//!
//! **Thin-binary contract:** `beskid_cli` parses Clap args and delegates here; new commands
//! should add a `commands/<name>.rs` wrapper plus library logic in the appropriate domain crate.

pub mod corelib;
pub mod diagnostics;
pub mod entrypoint;
pub mod logging;
pub mod pipeline;
pub mod prompt;
pub mod registry;
pub mod session;
pub mod shell;
pub mod toolchain;
pub mod tui;

pub use corelib::{CorelibProvisioning, ensure_bundled_corelib};
pub use diagnostics::{
    format_diagnostic, format_report, print_pretty_parse_error, print_pretty_pest_error, print_report,
    print_semantic_diagnostics, report_from_anyhow,
};
pub use entrypoint::{COMPILER_STACK_SIZE, compiler_stack_size, run_on_compiler_stack};
pub use logging::init as init_logging;
pub use pipeline::{
    CliInputPipelineOptions, CliPipeline, CliProjectPipelineOptions, CliResolveOptions, PipelineProgressKind,
    resolve_input_with_cli_pipeline, resolve_input_with_cli_pipeline_kind, resolve_project_with_cli_pipeline,
    use_cli_spinner,
};
pub use registry::{
    RegistryConnectConfig, build_pckg_client, is_network_error, latest_non_yanked, parse_package_selector,
    pckg_to_anyhow, pick_version, tokio_runtime,
};
pub use session::{CommandSession, ResolveInputArgs, SemanticGateOptions};
pub use shell::{
    BeskidWidget, BoardLayout, BoardV2Doc, CommandItem, HiLayoutState, ShellAction, ShellHost, ShellScope,
    WidgetDescriptor, WidgetMeta, WidgetRegistrar, WidgetRegistry,
};
