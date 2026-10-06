//! Host linker and static-archive integration (`cc` / `cl`, `ar`, `libtool`, version scripts).

mod common;
mod macos;
mod orchestration;
mod policy;
mod tool;
mod unix;
mod windows;

use std::path::PathBuf;

use crate::api::{BuildOutputKind, LinkMode};

pub use common::canonical_link_library_name;
#[cfg(test)]
pub(crate) use orchestration::unix_link_command;
pub use orchestration::{link, link_with_control};
pub use tool::{LinkToolInvocation, LinkToolReceipt, run_link_tool};

/// Exact runtime profile selected by validated runtime preparation.
#[derive(Debug, Clone)]
pub enum RuntimeLinkInput {
    CanonicalStatic { archive: PathBuf },
    GlueSharedProviderV1 { link_path: PathBuf, shared_library_path: PathBuf },
}
impl RuntimeLinkInput {
    pub fn path(&self) -> &std::path::Path {
        match self {
            Self::CanonicalStatic { archive } => archive,
            Self::GlueSharedProviderV1 { link_path, .. } => link_path,
        }
    }
    pub(crate) fn static_archive(&self) -> crate::error::AotResult<&std::path::Path> {
        match self {
            Self::CanonicalStatic { archive } => Ok(archive),
            Self::GlueSharedProviderV1 { .. } => Err(crate::error::AotError::InvalidRequest {
                message: "shared runtime provider cannot be merged into a static archive".into(),
            }),
        }
    }
}
impl TryFrom<&crate::runtime::RuntimeArtifact> for RuntimeLinkInput {
    type Error = crate::error::AotError;
    fn try_from(runtime: &crate::runtime::RuntimeArtifact) -> Result<Self, Self::Error> {
        use crate::runtime::RuntimeLinkage;
        match runtime.linkage {
            RuntimeLinkage::CanonicalStatic
                if runtime.shared_library_path.is_none() && runtime.owner_issuer_source_sha256.is_none() =>
            {
                Ok(Self::CanonicalStatic { archive: runtime.link_path.clone() })
            }
            RuntimeLinkage::GlueSharedProviderV1 => {
                let shared_library_path =
                    runtime.shared_library_path.clone().ok_or_else(|| Self::Error::InvalidRequest {
                        message: "shared runtime profile has no validated provider payload".into(),
                    })?;
                if runtime.owner_issuer_source_sha256.is_none() {
                    return Err(Self::Error::InvalidRequest {
                        message: "shared runtime profile has no checked owner issuer".into(),
                    });
                }
                Ok(Self::GlueSharedProviderV1 { link_path: runtime.link_path.clone(), shared_library_path })
            }
            _ => Err(Self::Error::InvalidRequest { message: "runtime profile and payload authority disagree".into() }),
        }
    }
}

/// Arguments for [`link`]: object path, optional typed runtime input, output shape, and exports.
#[derive(Debug, Clone)]
pub struct LinkRequest {
    pub target_triple: Option<String>,
    pub output_kind: BuildOutputKind,
    pub output_path: PathBuf,
    pub object_path: PathBuf,
    /// Additional native object files that must be present in every linked/archive artifact.
    pub additional_object_paths: Vec<PathBuf>,
    pub runtime: Option<RuntimeLinkInput>,
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
    pub tool_receipt: Option<LinkToolReceipt>,
    pub output_path: PathBuf,
    pub command_line: String,
    pub exported_symbols: Vec<String>,
}
