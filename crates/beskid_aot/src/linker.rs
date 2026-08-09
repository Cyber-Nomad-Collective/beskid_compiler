//! Host linker and static-archive integration (`cc` / `cl`, `ar`, `libtool`, version scripts).

mod common;
mod macos;
mod orchestration;
mod policy;
mod unix;
mod windows;

use std::path::PathBuf;

use crate::api::{BuildOutputKind, LinkMode};

pub use orchestration::link;

/// Arguments for [`link`]: object path, optional runtime archive, output shape, and exports.
#[derive(Debug, Clone)]
pub struct LinkRequest {
    pub target_triple: Option<String>,
    pub output_kind: BuildOutputKind,
    pub output_path: PathBuf,
    pub object_path: PathBuf,
    /// Additional native object files that must be present in every linked/archive artifact.
    pub additional_object_paths: Vec<PathBuf>,
    pub runtime_staticlib: Option<PathBuf>,
    pub host_staticlib: Option<PathBuf>,
    pub entrypoint_symbol: String,
    pub exported_symbols: Vec<String>,
    pub link_mode: LinkMode,
    pub verbose: bool,
    pub external_libraries: Vec<String>,
    pub library_search_paths: Vec<PathBuf>,
}

/// Successful link or archive merge: output path, echoed command line, and export list carried through.
#[derive(Debug, Clone)]
pub struct LinkResult {
    pub output_path: PathBuf,
    pub command_line: String,
    pub exported_symbols: Vec<String>,
}
