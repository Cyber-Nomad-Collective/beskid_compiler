//! OSC 8 terminal hyperlinks (`file://` → editor on click).

use std::env;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

/// Location of a test definition for terminal link targets.
#[derive(Debug, Clone)]
pub struct FileLineLink {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
}

/// Whether OSC 8 links are emitted (TTY stderr, not plain, hyperlinks not disabled).
pub fn hyperlinks_enabled(plain: bool) -> bool {
    !plain && stderr_is_tty() && env::var_os("BESKID_NO_HYPERLINKS").is_none() && env::var_os("NO_HYPERLINKS").is_none()
}

fn stderr_is_tty() -> bool {
    std::io::stderr().is_terminal()
}

/// Build a `file://` URI for VS Code / Cursor / iTerm-style editors (`path:line:column`).
pub fn file_line_uri(path: &Path, line: usize, column: usize) -> String {
    let abs = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let path_str = abs.display().to_string();
    format!("file://{path_str}:{line}:{column}")
}

/// Wrap `label` in an OSC 8 hyperlink to `uri`.
pub fn osc8_link(uri: &str, label: &str) -> String {
    format!("\x1b]8;;{uri}\x1b\\{label}\x1b]8;;\x1b\\")
}

/// Hyperlinked `label` when enabled; otherwise `label` unchanged.
pub fn maybe_link_label(link: Option<&FileLineLink>, label: &str, plain: bool) -> String {
    let Some(loc) = link else {
        return label.to_string();
    };
    if !hyperlinks_enabled(plain) {
        return label.to_string();
    }
    let uri = file_line_uri(&loc.path, loc.line, loc.column);
    osc8_link(&uri, label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osc8_wraps_label() {
        let out = osc8_link("file:///tmp/a.bd:2:3", "my_test");
        assert!(out.contains("file:///tmp/a.bd:2:3"));
        assert!(out.contains("my_test"));
        assert!(out.starts_with("\x1b]8;;"));
    }
}
