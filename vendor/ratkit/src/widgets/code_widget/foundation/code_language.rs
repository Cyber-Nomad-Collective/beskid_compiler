//! Supported source language identifiers.

/// Normalized language identifier for code rendering and outlines.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CodeLanguage {
    /// Rust source.
    Rust,
    /// TypeScript source.
    TypeScript,
    /// JavaScript source.
    JavaScript,
    /// Python source.
    Python,
    /// Shell script source.
    Shell,
    /// JSON source.
    Json,
    /// TOML source.
    Toml,
    /// Markdown source.
    Markdown,
    /// YAML source.
    Yaml,
    /// Known string not mapped to an outline extractor.
    Other(String),
    /// Unknown language.
    Unknown,
}

impl CodeLanguage {
    /// Creates a normalized language from a token.
    pub fn from_token(token: &str) -> Self {
        match token.to_ascii_lowercase().as_str() {
            "rs" | "rust" => Self::Rust,
            "ts" | "tsx" | "typescript" => Self::TypeScript,
            "js" | "jsx" | "javascript" | "node" => Self::JavaScript,
            "py" | "python" | "python3" => Self::Python,
            "sh" | "bash" | "zsh" | "shell" => Self::Shell,
            "json" => Self::Json,
            "toml" => Self::Toml,
            "md" | "markdown" => Self::Markdown,
            "yaml" | "yml" => Self::Yaml,
            "" => Self::Unknown,
            other => Self::Other(other.to_string()),
        }
    }

    /// Returns the token used by syntect.
    pub fn syntect_token(&self) -> Option<&str> {
        match self {
            Self::Rust => Some("rust"),
            Self::TypeScript => Some("ts"),
            Self::JavaScript => Some("js"),
            Self::Python => Some("python"),
            Self::Shell => Some("bash"),
            Self::Json => Some("json"),
            Self::Toml => Some("toml"),
            Self::Markdown => Some("markdown"),
            Self::Yaml => Some("yaml"),
            Self::Other(token) => Some(token.as_str()),
            Self::Unknown => None,
        }
    }

    /// Returns a display label for the language.
    pub fn label(&self) -> &str {
        self.syntect_token().unwrap_or("plain text")
    }
}
