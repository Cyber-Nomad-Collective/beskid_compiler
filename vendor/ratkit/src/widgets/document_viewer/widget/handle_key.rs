//! Keyboard navigation for shared document viewers.

use crossterm::event::{KeyCode, KeyEvent};

use crate::widgets::document_viewer::state::{ScrollState, VimState};

/// Applies a keyboard event to shared viewer scroll state.
pub fn handle_viewer_key(key: KeyEvent, scroll: &mut ScrollState, vim: &mut VimState) -> bool {
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            scroll.line_down();
            vim.clear();
            true
        }
        KeyCode::Up | KeyCode::Char('k') => {
            scroll.line_up();
            vim.clear();
            true
        }
        KeyCode::PageDown | KeyCode::Char('d') => {
            scroll.move_current_down(10);
            vim.clear();
            true
        }
        KeyCode::PageUp | KeyCode::Char('u') => {
            scroll.move_current_up(10);
            vim.clear();
            true
        }
        KeyCode::Char('g') if vim.pending_g => {
            scroll.scroll_to(0);
            vim.clear();
            true
        }
        KeyCode::Char('g') => {
            vim.pending_g = true;
            true
        }
        KeyCode::Char('G') => {
            scroll.scroll_to_bottom();
            vim.clear();
            true
        }
        _ => {
            vim.clear();
            false
        }
    }
}
