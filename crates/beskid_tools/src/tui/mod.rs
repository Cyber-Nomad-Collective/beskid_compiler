//! Unified Beskid terminal shell (ratkit-style Runner + CoordinatorApp on ratatui 0.30).
//!
//! Pipeline compile UI, test overlays, and pckg/template views share one runtime.

pub mod app;
pub mod kit;
pub mod effects;
pub mod overlay_chrome;
pub mod shell_fx;
pub mod input;
pub mod layout;
pub mod message;
pub mod panes;
pub mod render;
pub mod screens;
pub mod session;
pub mod shell;
pub mod views;
pub mod widgets;

pub use message::ShellMessage;
pub use session::ShellSession;

/// Open the interactive new-project template picker (blocking until quit).
pub fn run_project_wizard() -> std::io::Result<()> {
    ShellSession::run_project_wizard()
}
pub use shell::focus::{FocusTarget, OverlayKind, PaneFocus};
pub use shell::state::{NavTarget, ShellState};
