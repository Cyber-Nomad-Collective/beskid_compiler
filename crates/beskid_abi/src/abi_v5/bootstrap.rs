//! Canonical runtime package identity, audit metadata, generated-source bootstrap, and header
//! rendering for the ABI-v5 manifest.

mod audit;
mod identity;
mod manifest;
mod render;
mod source_contract;

pub use audit::RuntimeAuditMetadata;
pub use identity::{
    CANONICAL_RUNTIME_PACKAGE_NAME, CANONICAL_RUNTIME_PACKAGE_PUBLISHER, RuntimePackageIdentity, TRAP_DIAGNOSTIC_PREFIX,
    TRAP_EXIT_STATUS, canonical_runtime_package,
};
pub use render::{render_runtime_asm_include, render_runtime_c_header};
