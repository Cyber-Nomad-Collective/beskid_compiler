//! Input events and routing results.

use crossterm::event::{KeyEvent, MouseEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputAction {
    None,
    Advance,
    /// Skip a blocked `wait_for` without opening the target overlay.
    SkipNav,
    Quit,
    Redraw,
}

#[derive(Debug, Clone)]
pub enum InputEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputResult {
    Handled,
    /// Navigation advanced (Space/Enter): open next overlay or dismiss summary.
    Advance,
    /// Skip a blocked `wait_for` (q/Esc on base layer).
    SkipNav,
    Bubble,
    CloseOverlay,
    Quit,
}
