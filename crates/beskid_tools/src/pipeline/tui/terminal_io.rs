//! Stderr terminal lifecycle (mirrors [`ratatui::try_init`] / [`ratatui::try_restore`]).
//!
//! Beskid renders the pipeline UI on **stderr** so stdout stays free for tool output.
//! Ratatui's built-in [`ratatui::init`] helpers target stdout; use this module for the
//! stderr [`CrosstermBackend`].

use std::io::{self, Stderr, stderr};
use std::sync::Once;

use crossterm::ExecutableCommand;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

/// `Terminal<CrosstermBackend<Stderr>>` — the pipeline build UI backend.
pub type StderrTerminal = Terminal<CrosstermBackend<Stderr>>;

static PANIC_HOOK: Once = Once::new();

/// Initialize raw mode, alternate screen, and mouse capture on stderr.
///
/// Installs a panic hook (once) that restores stderr terminal state before chaining
/// to the previous hook, matching Ratatui's [`ratatui::try_init`] behavior.
pub fn try_init_stderr() -> io::Result<StderrTerminal> {
    install_stderr_panic_hook();
    enable_raw_mode()?;
    stderr().execute(EnterAlternateScreen)?;
    stderr().execute(EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stderr());
    Terminal::new(backend)
}

/// Leave alternate screen, disable mouse capture, and disable raw mode on stderr.
pub fn try_restore_stderr() -> io::Result<()> {
    let _ = stderr().execute(DisableMouseCapture);
    disable_raw_mode()?;
    stderr().execute(LeaveAlternateScreen)?;
    Ok(())
}

/// Best-effort stderr restore; errors are ignored (same contract as [`ratatui::restore`]).
pub fn restore_stderr() {
    if let Err(err) = try_restore_stderr() {
        eprintln!("Failed to restore terminal: {err}");
    }
}

fn install_stderr_panic_hook() {
    PANIC_HOOK.call_once(|| {
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore_stderr();
            hook(info);
        }));
    });
}
