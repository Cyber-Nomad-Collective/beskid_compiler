//! Template engine diagnostics (**E1901–E1999**).

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("E1901: template manifest missing or invalid: {0}")]
    InvalidManifest(String),

    #[error("E1902: package kind is not `template`: {package_id}")]
    NotTemplatePackage { package_id: String },

    #[error("E1903: required symbol `{symbol_id}` not provided")]
    RequiredSymbol { symbol_id: String },

    #[error("E1904: output path conflict: {path}")]
    OutputConflict { path: PathBuf },

    #[error("E1905: item template outside project root: {path}")]
    ItemOutsideProject { path: PathBuf },

    #[error("E1906: GUID replacement incomplete: {guid}")]
    GuidReplacement { guid: String },

    #[error("E1907: git template source failed: {0}")]
    GitSource(String),

    #[error("E1908: workspace template invalid: {0}")]
    WorkspaceInvalid(String),

    #[error("E1999: {0}")]
    Internal(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type TemplateResult<T> = Result<T, TemplateError>;
