//! Host linker and static-archive integration (`cc` / `cl`, `ar`, `libtool`, version scripts).

mod common;
mod macos;
mod orchestration;
mod policy;
mod unix;
mod windows;

use std::path::PathBuf;

use crate::api::{BuildOutputKind, LinkMode};

pub use common::canonical_link_library_name;
pub use orchestration::link;
#[cfg(test)]
pub(crate) use orchestration::unix_link_command;

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
    /// Libraries of optional `[Extern]` contracts. Each is linked only when the linker can find
    /// it; otherwise its symbols stay weak undefined references that resolve to null.
    pub optional_libraries: Vec<OptionalLinkLibrary>,
    pub library_search_paths: Vec<PathBuf>,
}

/// A library named by an `[Extern(..., Optional:true)]` contract, with the symbols the object
/// references from it as weak undefined symbols.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionalLinkLibrary {
    pub library: String,
    pub symbols: Vec<String>,
}

/// Successful link or archive merge: output path, echoed command line, and export list carried through.
#[derive(Debug, Clone)]
pub struct LinkResult {
    pub output_path: PathBuf,
    pub command_line: String,
    pub exported_symbols: Vec<String>,
}
