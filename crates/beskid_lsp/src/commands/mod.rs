//! `workspace/executeCommand` routing for Beskid extension commands.

pub mod pckg_registry;
pub mod project_explorer;
pub mod symbol_documentation;

pub use pckg_registry::PckgRegistryState;
pub use project_explorer::{
    PROJECT_EXPLORER_COMMANDS, focused_project_from_configuration, focused_project_from_value,
    handle_project_explorer_command,
};
