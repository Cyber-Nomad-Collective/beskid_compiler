//! Platform-aware shortcut labels and key matching (macOS terminals vs others).
//!
//! macOS terminals often swallow ⌘ (Super) for system/app shortcuts (⌘M minimizes,
//! ⌘P opens editor quick-open). Hi uses **Ctrl** chords in the terminal on all platforms.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// True when running on macOS (Terminal.app, iTerm, VS Code integrated terminal, etc.).
pub fn is_macos() -> bool {
    std::env::consts::OS == "macos"
}

/// Command palette binding shown in UI chrome.
pub fn palette_label() -> &'static str {
    "Ctrl+P"
}

/// Palette hint including the `:` alternative.
pub fn palette_hint() -> String {
    "Ctrl+P / :".into()
}

/// Top menu binding shown in UI chrome.
pub fn menu_label() -> &'static str {
    if is_macos() { "Ctrl+M" } else { "F10" }
}

/// Full menu hint (macOS keeps Fn+F10 as a secondary because F-keys are often media keys).
pub fn menu_hint() -> String {
    if is_macos() {
        "Ctrl+M / Fn+F10".into()
    } else {
        "F10".into()
    }
}

/// Open the command palette (`Ctrl+P` / `:`).
pub fn opens_palette(key: &KeyEvent) -> bool {
    if key.code == KeyCode::Char(':') {
        return true;
    }
    matches!(key.code, KeyCode::Char('p') | KeyCode::Char('P'))
        && key.modifiers.contains(KeyModifiers::CONTROL)
}

/// Toggle the pinned top menu (`F10`; on macOS also `Ctrl+M`).
pub fn toggles_menu(key: &KeyEvent) -> bool {
    if key.code == KeyCode::F(10) {
        return true;
    }
    matches!(key.code, KeyCode::Char('m') | KeyCode::Char('M'))
        && key.modifiers.contains(KeyModifiers::CONTROL)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEventKind;

    fn key(modifiers: KeyModifiers, code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    #[test]
    fn colon_always_opens_palette() {
        assert!(opens_palette(&key(KeyModifiers::NONE, KeyCode::Char(':'))));
    }

    #[test]
    fn control_p_opens_palette() {
        assert!(opens_palette(&key(
            KeyModifiers::CONTROL,
            KeyCode::Char('p'),
        )));
    }

    #[test]
    fn super_p_does_not_open_palette() {
        assert!(!opens_palette(&key(
            KeyModifiers::SUPER,
            KeyCode::Char('p'),
        )));
    }

    #[test]
    fn control_m_toggles_menu() {
        assert!(toggles_menu(&key(
            KeyModifiers::CONTROL,
            KeyCode::Char('m'),
        )));
    }

    #[test]
    fn super_m_does_not_toggle_menu() {
        assert!(!toggles_menu(&key(
            KeyModifiers::SUPER,
            KeyCode::Char('m'),
        )));
    }
}
