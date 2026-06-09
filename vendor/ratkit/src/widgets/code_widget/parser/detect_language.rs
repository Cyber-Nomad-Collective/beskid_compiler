//! Language detection for source files.

use std::path::Path;

use crate::widgets::code_widget::foundation::CodeLanguage;

/// Detects a source language from override, file path, and shebang.
pub fn detect_language(
    path: Option<&Path>,
    content: &str,
    override_language: Option<&str>,
) -> CodeLanguage {
    if let Some(language) = override_language {
        return CodeLanguage::from_token(language);
    }

    if let Some(language) = detect_from_shebang(content) {
        return language;
    }

    path.and_then(|path| path.extension())
        .and_then(|extension| extension.to_str())
        .map(CodeLanguage::from_token)
        .unwrap_or(CodeLanguage::Unknown)
}

/// Detects a language from a Unix shebang.
fn detect_from_shebang(content: &str) -> Option<CodeLanguage> {
    let first_line = content.lines().next()?.trim();
    if !first_line.starts_with("#!") {
        return None;
    }

    let shebang = first_line.trim_start_matches("#!");
    for token in shebang.split(|character: char| character == '/' || character.is_whitespace()) {
        let language = CodeLanguage::from_token(token);
        if is_shebang_language(&language) {
            return Some(language);
        }
    }
    None
}

/// Returns true for languages that should be trusted from shebang tokens.
fn is_shebang_language(language: &CodeLanguage) -> bool {
    matches!(
        language,
        CodeLanguage::Python | CodeLanguage::JavaScript | CodeLanguage::Shell
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_rust_from_extension() {
        let language = detect_language(Some(Path::new("src/main.rs")), "", None);
        assert_eq!(language, CodeLanguage::Rust);
    }

    #[test]
    fn detects_python_from_shebang() {
        let language = detect_language(None, "#!/usr/bin/env python3\nprint('x')", None);
        assert_eq!(language, CodeLanguage::Python);
    }

    #[test]
    fn explicit_language_overrides_path() {
        let language = detect_language(Some(Path::new("main.py")), "", Some("rust"));
        assert_eq!(language, CodeLanguage::Rust);
    }
}
