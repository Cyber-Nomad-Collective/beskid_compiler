//! Template engine diagnostics (**E2001–E2099**).

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("E2001: template manifest missing or invalid: {0}")]
    InvalidManifest(String),

    #[error("E2002: package kind is not `template`: {package_id}")]
    NotTemplatePackage { package_id: String },

    #[error("E2003: required symbol `{symbol_id}` not provided")]
    RequiredSymbol { symbol_id: String },

    #[error("E2004: output path conflict: {path}")]
    OutputConflict { path: PathBuf },

    #[error("E2005: item template outside project root: {path}")]
    ItemOutsideProject { path: PathBuf },

    #[error("E2006: GUID replacement incomplete: {guid}")]
    GuidReplacement { guid: String },

    #[error("E2007: git template source failed: {0}")]
    GitSource(String),

    #[error("E2008: workspace template invalid: {0}")]
    WorkspaceInvalid(String),

    #[error("E2099: {0}")]
    Internal(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type TemplateResult<T> = Result<T, TemplateError>;
