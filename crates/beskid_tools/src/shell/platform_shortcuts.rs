//! Platform-aware shortcut labels and key matching (macOS terminals vs others).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// True when running on macOS (Terminal.app, iTerm, VS Code integrated terminal, etc.).
pub fn is_macos() -> bool {
    std::env::consts::OS == "macos"
}

/// Command palette binding shown in UI chrome.
pub fn palette_label() -> &'static str {
    if is_macos() { "⌘P" } else { "Ctrl+P" }
}

/// Palette hint including the `:` alternative.
pub fn palette_hint() -> String {
    if is_macos() {
        "⌘P / :".into()
    } else {
        "Ctrl+P / :".into()
    }
}

/// Top menu binding shown in UI chrome.
pub fn menu_label() -> &'static str {
    if is_macos() { "Fn+F10" } else { "F10" }
}

/// Full menu hint (macOS adds ⌘M because F-keys are often media keys).
pub fn menu_hint() -> String {
    if is_macos() {
        "⌘M / Fn+F10".into()
    } else {
        "F10".into()
    }
}

/// Open the command palette (`Ctrl+P` / `⌘P` / `:`).
pub fn opens_palette(key: &KeyEvent) -> bool {
    if key.code == KeyCode::Char(':') {
        return true;
    }
    let is_p = matches!(key.code, KeyCode::Char('p') | KeyCode::Char('P'));
    if !is_p {
        return false;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return true;
    }
    is_macos() && key.modifiers.contains(KeyModifiers::SUPER)
}

/// Toggle the pinned top menu (`F10`; on macOS also `⌘M`).
pub fn toggles_menu(key: &KeyEvent) -> bool {
    if key.code == KeyCode::F(10) {
        return true;
    }
    is_macos()
        && matches!(key.code, KeyCode::Char('m') | KeyCode::Char('M'))
        && key.modifiers.contains(KeyModifiers::SUPER)
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
}
