//! User-configurable shell shortcut bindings (persisted in tools.bsol).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::platform_shortcuts;
use super::settings::{ToolSettingsRegistry, ToolsConfig, get_value, set_value};

pub const TOOL_ID: &str = "shell";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyChord {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

#[derive(Debug, Clone, Copy)]
pub struct BindableAction {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub config_key: &'static str,
}

pub const BINDABLE_ACTIONS: &[BindableAction] = &[
    BindableAction {
        id: "palette",
        label: "Command palette",
        description: "Open fuzzy command search (`:` also works)",
        config_key: "bind_palette",
    },
    BindableAction {
        id: "menu",
        label: "Top menu",
        description: "Toggle pinned workflow menu",
        config_key: "bind_menu",
    },
    BindableAction {
        id: "help",
        label: "Shortcut help",
        description: "Toggle footer shortcut overlay",
        config_key: "bind_help",
    },
    BindableAction {
        id: "quit",
        label: "Quit",
        description: "Exit beskid hi",
        config_key: "bind_quit",
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutBindings {
    pub palette: KeyChord,
    pub menu: KeyChord,
    pub help: KeyChord,
    pub quit: KeyChord,
}

impl ShortcutBindings {
    pub fn platform_defaults() -> Self {
        let palette = KeyChord {
            code: KeyCode::Char('p'),
            modifiers: KeyModifiers::CONTROL,
        };
        let menu = if platform_shortcuts::is_macos() {
            KeyChord {
                code: KeyCode::Char('m'),
                modifiers: KeyModifiers::CONTROL,
            }
        } else {
            KeyChord {
                code: KeyCode::F(10),
                modifiers: KeyModifiers::NONE,
            }
        };
        Self {
            palette,
            menu,
            help: KeyChord {
                code: KeyCode::Char('?'),
                modifiers: KeyModifiers::NONE,
            },
            quit: KeyChord {
                code: KeyCode::Char('q'),
                modifiers: KeyModifiers::NONE,
            },
        }
    }

    pub fn load(config: &ToolsConfig, registry: &ToolSettingsRegistry) -> Self {
        let defaults = Self::platform_defaults();
        Self {
            palette: load_chord(config, registry, "bind_palette", defaults.palette),
            menu: load_chord(config, registry, "bind_menu", defaults.menu),
            help: load_chord(config, registry, "bind_help", defaults.help),
            quit: load_chord(config, registry, "bind_quit", defaults.quit),
        }
    }

    pub fn save(&self, config: &mut ToolsConfig) {
        for action in BINDABLE_ACTIONS {
            let chord = self.chord_for(action.id);
            set_value(
                config,
                TOOL_ID,
                action.config_key,
                encode_chord(chord),
            );
        }
    }

    pub fn reset_to_defaults(&mut self) {
        *self = Self::platform_defaults();
    }

    pub fn chord_for(&self, action_id: &str) -> KeyChord {
        match action_id {
            "palette" => self.palette,
            "menu" => self.menu,
            "help" => self.help,
            "quit" => self.quit,
            _ => self.palette,
        }
    }

    pub fn set_chord(&mut self, action_id: &str, chord: KeyChord) {
        match action_id {
            "palette" => self.palette = chord,
            "menu" => self.menu = chord,
            "help" => self.help = chord,
            "quit" => self.quit = chord,
            _ => {}
        }
    }

    pub fn label_for(&self, action_id: &str) -> String {
        display_chord(self.chord_for(action_id))
    }

    pub fn opens_palette(&self, key: &KeyEvent) -> bool {
        if key.code == KeyCode::Char(':') {
            return true;
        }
        chord_matches(&self.palette, key)
    }

    pub fn toggles_menu(&self, key: &KeyEvent) -> bool {
        if chord_matches(&self.menu, key) {
            return true;
        }
        // Fn+F10 alternate on macOS (F-keys are often media keys without Fn).
        platform_shortcuts::is_macos() && key.code == KeyCode::F(10)
    }

    pub fn toggles_help(&self, key: &KeyEvent) -> bool {
        chord_matches(&self.help, key)
    }

    pub fn quits(&self, key: &KeyEvent) -> bool {
        chord_matches(&self.quit, key)
    }

    pub fn palette_hint(&self) -> String {
        format!("{} / :", display_chord(self.palette))
    }

    pub fn menu_hint(&self) -> String {
        if platform_shortcuts::is_macos() {
            format!("{} / Fn+F10", display_chord(self.menu))
        } else {
            display_chord(self.menu)
        }
    }
}

fn load_chord(
    config: &ToolsConfig,
    registry: &ToolSettingsRegistry,
    key: &str,
    default: KeyChord,
) -> KeyChord {
    let raw = get_value(config, registry, TOOL_ID, key);
    if raw.is_empty() {
        return default;
    }
    if platform_shortcuts::is_macos() {
        match (key, raw.as_str()) {
            ("bind_palette", "super+p" | "cmd+p" | "command+p") => return default,
            ("bind_menu", "super+m" | "cmd+m" | "command+m") => return default,
            _ => {}
        }
    }
    parse_chord(&raw).unwrap_or(default)
}

pub fn chord_from_key(key: &KeyEvent) -> KeyChord {
    KeyChord {
        code: key.code,
        modifiers: key.modifiers
            & (KeyModifiers::CONTROL
                | KeyModifiers::ALT
                | KeyModifiers::SHIFT
                | KeyModifiers::SUPER),
    }
}

pub fn chord_matches(chord: &KeyChord, key: &KeyEvent) -> bool {
    let mods = key.modifiers
        & (KeyModifiers::CONTROL
            | KeyModifiers::ALT
            | KeyModifiers::SHIFT
            | KeyModifiers::SUPER);
    chord.code == key.code && chord.modifiers == mods
}

pub fn encode_chord(chord: KeyChord) -> String {
    let mut parts = Vec::new();
    if chord.modifiers.contains(KeyModifiers::SUPER) {
        parts.push("super");
    }
    if chord.modifiers.contains(KeyModifiers::CONTROL) {
        parts.push("ctrl");
    }
    if chord.modifiers.contains(KeyModifiers::ALT) {
        parts.push("alt");
    }
    if chord.modifiers.contains(KeyModifiers::SHIFT) {
        parts.push("shift");
    }
    let key_token = code_to_token(chord.code);
    parts.push(key_token.as_str());
    parts.join("+")
}

pub fn parse_chord(raw: &str) -> Result<KeyChord, String> {
    let trimmed = raw.trim().to_ascii_lowercase();
    if trimmed.is_empty() {
        return Err("empty chord".into());
    }
    let segments: Vec<&str> = trimmed.split('+').collect();
    if segments.is_empty() {
        return Err("empty chord".into());
    }
    let key_token = segments[segments.len() - 1];
    let mut modifiers = KeyModifiers::NONE;
    for token in &segments[..segments.len() - 1] {
        match *token {
            "ctrl" | "control" => modifiers |= KeyModifiers::CONTROL,
            "alt" | "option" => modifiers |= KeyModifiers::ALT,
            "shift" => modifiers |= KeyModifiers::SHIFT,
            "super" | "cmd" | "command" | "meta" => modifiers |= KeyModifiers::SUPER,
            other => return Err(format!("unknown modifier `{other}`")),
        }
    }
    let code = token_to_code(key_token)?;
    Ok(KeyChord { code, modifiers })
}

pub fn display_chord(chord: KeyChord) -> String {
    let mut out = String::new();
    let mac = platform_shortcuts::is_macos();
    if chord.modifiers.contains(KeyModifiers::SUPER) {
        out.push_str(if mac { "⌘" } else { "Super+" });
    }
    if chord.modifiers.contains(KeyModifiers::CONTROL) {
        out.push_str(if mac { "⌃" } else { "Ctrl+" });
    }
    if chord.modifiers.contains(KeyModifiers::ALT) {
        out.push_str(if mac { "⌥" } else { "Alt+" });
    }
    if chord.modifiers.contains(KeyModifiers::SHIFT) {
        out.push_str(if mac { "⇧" } else { "Shift+" });
    }
    out.push_str(&display_code(chord.code));
    out
}

fn code_to_token(code: KeyCode) -> String {
    match code {
        KeyCode::Char(c) => c.to_string(),
        KeyCode::F(n) => format!("f{n}"),
        KeyCode::Enter => "enter".into(),
        KeyCode::Esc => "esc".into(),
        KeyCode::Tab => "tab".into(),
        KeyCode::Backspace => "backspace".into(),
        KeyCode::Delete => "delete".into(),
        KeyCode::Home => "home".into(),
        KeyCode::End => "end".into(),
        KeyCode::PageUp => "pageup".into(),
        KeyCode::PageDown => "pagedown".into(),
        KeyCode::Up => "up".into(),
        KeyCode::Down => "down".into(),
        KeyCode::Left => "left".into(),
        KeyCode::Right => "right".into(),
        other => format!("{other:?}").to_ascii_lowercase(),
    }
}

fn token_to_code(token: &str) -> Result<KeyCode, String> {
    match token {
        ":" | "colon" => Ok(KeyCode::Char(':')),
        "?" => Ok(KeyCode::Char('?')),
        "enter" | "return" => Ok(KeyCode::Enter),
        "esc" | "escape" => Ok(KeyCode::Esc),
        "tab" => Ok(KeyCode::Tab),
        "backspace" => Ok(KeyCode::Backspace),
        "delete" => Ok(KeyCode::Delete),
        "home" => Ok(KeyCode::Home),
        "end" => Ok(KeyCode::End),
        "pageup" => Ok(KeyCode::PageUp),
        "pagedown" => Ok(KeyCode::PageDown),
        "up" => Ok(KeyCode::Up),
        "down" => Ok(KeyCode::Down),
        "left" => Ok(KeyCode::Left),
        "right" => Ok(KeyCode::Right),
        "space" => Ok(KeyCode::Char(' ')),
        s if s.len() == 1 => {
            let c = s.chars().next().unwrap();
            Ok(KeyCode::Char(c))
        }
        s if s.starts_with('f') && s.len() > 1 => {
            let n: u8 = s[1..]
                .parse()
                .map_err(|_| format!("invalid function key `{s}`"))?;
            Ok(KeyCode::F(n))
        }
        other => Err(format!("unknown key `{other}`")),
    }
}

fn display_code(code: KeyCode) -> String {
    match code {
        KeyCode::Char(c) => c.to_string(),
        KeyCode::F(n) => format!("F{n}"),
        KeyCode::Enter => "Enter".into(),
        KeyCode::Esc => "Esc".into(),
        KeyCode::Tab => "Tab".into(),
        KeyCode::Backspace => "Backspace".into(),
        KeyCode::Delete => "Delete".into(),
        KeyCode::Home => "Home".into(),
        KeyCode::End => "End".into(),
        KeyCode::PageUp => "PageUp".into(),
        KeyCode::PageDown => "PageDown".into(),
        KeyCode::Up => "Up".into(),
        KeyCode::Down => "Down".into(),
        KeyCode::Left => "Left".into(),
        KeyCode::Right => "Right".into(),
        other => format!("{other:?}"),
    }
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
    fn parse_encode_roundtrip() {
        let chord = KeyChord {
            code: KeyCode::Char('p'),
            modifiers: KeyModifiers::CONTROL,
        };
        let encoded = encode_chord(chord);
        assert_eq!(encoded, "ctrl+p");
        assert_eq!(parse_chord(&encoded).unwrap(), chord);
    }

    #[test]
    fn colon_always_opens_palette() {
        let bindings = ShortcutBindings::platform_defaults();
        assert!(bindings.opens_palette(&key(KeyModifiers::NONE, KeyCode::Char(':'))));
    }

    #[test]
    fn save_load_roundtrip() {
        let registry = ToolSettingsRegistry::with_builtins();
        let mut config = ToolsConfig::default();
        let mut bindings = ShortcutBindings::platform_defaults();
        bindings.help = KeyChord {
            code: KeyCode::Char('h'),
            modifiers: KeyModifiers::CONTROL,
        };
        bindings.save(&mut config);
        let loaded = ShortcutBindings::load(&config, &registry);
        assert_eq!(loaded.help, bindings.help);
    }
}
