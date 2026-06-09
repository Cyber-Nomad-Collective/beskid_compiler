//! Lightweight code symbol extraction.

use crate::widgets::code_widget::foundation::{CodeLanguage, CodeOutlineItem};

/// Extracts deterministic outline symbols without language-server integration.
pub fn extract_symbol_outline(content: &str, language: &CodeLanguage) -> Vec<CodeOutlineItem> {
    match language {
        CodeLanguage::Rust => extract_rust_symbols(content),
        CodeLanguage::TypeScript | CodeLanguage::JavaScript => extract_script_symbols(content),
        CodeLanguage::Python => extract_python_symbols(content),
        _ => Vec::new(),
    }
}

/// Extracts Rust symbols from source text.
fn extract_rust_symbols(content: &str) -> Vec<CodeOutlineItem> {
    content
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let trimmed = strip_rust_visibility(line.trim_start());
            rust_symbol(trimmed).map(|(kind, name)| CodeOutlineItem::new(name, index + 1, 0, kind))
        })
        .collect()
}

/// Removes common Rust visibility and qualifier prefixes.
fn strip_rust_visibility(line: &str) -> &str {
    line.strip_prefix("pub ")
        .or_else(|| line.strip_prefix("pub(crate) "))
        .or_else(|| line.strip_prefix("async "))
        .unwrap_or(line)
}

/// Extracts one Rust symbol from a trimmed line.
fn rust_symbol(line: &str) -> Option<(&'static str, String)> {
    for (prefix, kind) in [
        ("fn ", "fn"),
        ("struct ", "struct"),
        ("enum ", "enum"),
        ("trait ", "trait"),
        ("mod ", "mod"),
    ] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return Some((kind, take_identifier(rest)));
        }
    }
    line.strip_prefix("impl ")
        .map(|rest| ("impl", format!("impl {}", take_impl_target(rest))))
}

/// Extracts TypeScript and JavaScript symbols from source text.
fn extract_script_symbols(content: &str) -> Vec<CodeOutlineItem> {
    content
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let trimmed = line
                .trim_start()
                .strip_prefix("export ")
                .unwrap_or(line.trim_start());
            script_symbol(trimmed)
                .map(|(kind, name)| CodeOutlineItem::new(name, index + 1, 0, kind))
        })
        .collect()
}

/// Extracts one TypeScript or JavaScript symbol from a trimmed line.
fn script_symbol(line: &str) -> Option<(&'static str, String)> {
    if let Some(rest) = line.strip_prefix("function ") {
        return Some(("function", take_identifier(rest)));
    }
    if let Some(rest) = line.strip_prefix("class ") {
        return Some(("class", take_identifier(rest)));
    }
    if let Some(rest) = line.strip_prefix("const ") {
        if rest.contains("=>") {
            return Some(("function", take_identifier(rest)));
        }
    }
    None
}

/// Extracts Python symbols from source text.
fn extract_python_symbols(content: &str) -> Vec<CodeOutlineItem> {
    content
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let trimmed = line.trim_start();
            let level = line.len().saturating_sub(trimmed.len()) / 4;
            if let Some(rest) = trimmed.strip_prefix("def ") {
                return Some(CodeOutlineItem::new(
                    take_identifier(rest),
                    index + 1,
                    level,
                    "def",
                ));
            }
            trimmed
                .strip_prefix("class ")
                .map(|rest| CodeOutlineItem::new(take_identifier(rest), index + 1, level, "class"))
        })
        .collect()
}

/// Takes an identifier-like prefix from source text.
fn take_identifier(rest: &str) -> String {
    rest.chars()
        .take_while(|character| character.is_alphanumeric() || *character == '_')
        .collect()
}

/// Takes a Rust impl target from source text.
fn take_impl_target(rest: &str) -> String {
    rest.split('{').next().unwrap_or(rest).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_rust_symbols() {
        let outline = extract_symbol_outline(
            "pub struct App;\nimpl App {\nfn run() {}",
            &CodeLanguage::Rust,
        );
        assert_eq!(outline[0].name, "App");
        assert_eq!(outline[1].kind, "impl");
        assert_eq!(outline[2].name, "run");
    }

    #[test]
    fn extracts_script_symbols() {
        let outline = extract_symbol_outline(
            "export const run = () => {};\nclass App {}",
            &CodeLanguage::TypeScript,
        );
        assert_eq!(outline.len(), 2);
        assert_eq!(outline[0].name, "run");
    }

    #[test]
    fn extracts_python_symbols() {
        let outline = extract_symbol_outline(
            "class App:\n    def run(self):\n        pass",
            &CodeLanguage::Python,
        );
        assert_eq!(outline[1].level, 1);
        assert_eq!(outline[1].kind, "def");
    }
}
