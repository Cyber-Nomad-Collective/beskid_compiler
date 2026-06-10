//! Tuirealm integration: stderr terminal adapter and shell event loops.

pub mod hi;
pub mod pipeline_component;
pub mod shell_event;
pub mod stderr_adapter;

pub use hi::run_hi;
pub use pipeline_component::{PipelineShellComponent, PipelineShellId, PipelineShellMsg};
pub use shell_event::{ShellOutcome, ShellRealmEvent, shell_event_from_realm};
pub use stderr_adapter::StderrTerminalAdapter;
