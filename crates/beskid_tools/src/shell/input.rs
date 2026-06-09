//! Shell input events (keyboard + mouse).

use crossterm::event::{KeyEvent, MouseEvent};

#[derive(Debug, Clone, Copy)]
pub enum ShellInput {
    Key(KeyEvent),
    Mouse(MouseEvent),
}
