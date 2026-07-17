//! Sole generation-safe analysis-to-codegen boundary.

use std::sync::Arc;

use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};
use beskid_queries::{AstNodeKey, Db, TypedProgram, node_kind};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CodegenInputError {
    #[error("codegen input has no AST roots")]
    MissingRoots,
    #[error("invalid ABI-v5 target metadata")]
    InvalidTarget,
    #[error("ABI-v5 manifest target does not match the codegen target")]
    ManifestTargetMismatch,
    #[error("ABI-v5 manifest differs from the canonical target contract")]
    ManifestDrift,
    #[error("typed program entry does not belong to its syntax assembly")]
    InvalidEntry,
    #[error("AST root is stale, foreign, or absent from the syntax assembly: {0:?}")]
    InvalidRoot(AstNodeKey),
}

/// Complete HIR-free input required before generated ISLE selection may begin.
pub struct CodegenInput<'db> {
    db: &'db dyn Db,
    typed_program: TypedProgram,
    roots: Arc<[AstNodeKey]>,
    target: TargetMetadata,
    abi_manifest: AbiManifestV5,
}

impl<'db> CodegenInput<'db> {
    pub fn new(
        db: &'db dyn Db,
        typed_program: TypedProgram,
        roots: Arc<[AstNodeKey]>,
        target: TargetMetadata,
        abi_manifest: AbiManifestV5,
    ) -> Result<Self, CodegenInputError> {
        if roots.is_empty() {
            return Err(CodegenInputError::MissingRoots);
        }
        target
            .validate()
            .map_err(|_| CodegenInputError::InvalidTarget)?;
        abi_manifest
            .validate()
            .map_err(|_| CodegenInputError::ManifestDrift)?;
        if abi_manifest.target != target {
            return Err(CodegenInputError::ManifestTargetMismatch);
        }
        if abi_manifest != AbiManifestV5::canonical_runtime(target.clone()) {
            return Err(CodegenInputError::ManifestDrift);
        }

        let entry_path = typed_program.entry.path(db);
        let entry_matches = typed_program
            .assembly
            .units()
            .iter()
            .any(|unit| paths_match(&unit.path, entry_path));
        if !entry_matches {
            return Err(CodegenInputError::InvalidEntry);
        }

        for root in roots.iter().copied() {
            let unit_path = root.unit.path(db);
            let belongs_to_assembly = typed_program
                .assembly
                .units()
                .iter()
                .any(|unit| paths_match(&unit.path, unit_path));
            if !belongs_to_assembly || !matches!(node_kind(db, root), Ok(Some(_))) {
                return Err(CodegenInputError::InvalidRoot(root));
            }
        }

        Ok(Self {
            db,
            typed_program,
            roots,
            target,
            abi_manifest,
        })
    }

    pub fn database(&self) -> &'db dyn Db {
        self.db
    }

    pub fn typed_program(&self) -> &TypedProgram {
        &self.typed_program
    }

    pub fn roots(&self) -> &[AstNodeKey] {
        &self.roots
    }

    pub fn target(&self) -> &TargetMetadata {
        &self.target
    }

    pub fn abi_manifest(&self) -> &AbiManifestV5 {
        &self.abi_manifest
    }

    /// Compiler-minted authority for direct ABI-v5 intrinsic imports.
    ///
    /// It is absent for every ordinary user program, including projects that imitate runtime
    /// paths or package metadata.
    pub fn runtime_intrinsic_capability(
        &self,
    ) -> Option<&std::sync::Arc<beskid_abi::runtime_source::RuntimeIntrinsicCapability>> {
        self.typed_program.runtime_intrinsic_capability.as_ref()
    }

    /// Compiler-minted authority for Corelib syscall service imports. This separate proof never
    /// grants canonical-runtime intrinsic authority to the Corelib facade.
    pub fn corelib_service_capability(
        &self,
    ) -> Option<&std::sync::Arc<beskid_abi::runtime_source::CorelibServiceCapability>> {
        self.typed_program.corelib_service_capability.as_ref()
    }

    /// Resolve one direct ABI-v5 intrinsic import through the canonical-source capability.
    ///
    /// A current node from a foreign unit, a stale node, a user program, and an undeclared
    /// manifest name all return `None`; callers must never substitute an extern fallback.
    pub fn runtime_intrinsic_for(
        &self,
        key: AstNodeKey,
        name: &str,
    ) -> Option<(u32, &beskid_abi::abi_v5::RuntimeIntrinsic)> {
        if !matches!(node_kind(self.db, key), Ok(Some(_))) {
            return None;
        }
        let unit_path = key.unit.path(self.db);
        let logical_path = self
            .typed_program
            .assembly
            .units()
            .iter()
            .find(|unit| paths_match(&unit.path, unit_path))?
            .logical_name
            .as_str();
        let capability = self.runtime_intrinsic_capability()?;
        let intrinsic = capability.intrinsic_for_source(logical_path, name)?;
        let index = self
            .abi_manifest
            .trusted_runtime_intrinsics
            .iter()
            .position(|candidate| candidate.name == intrinsic.name)?;
        Some((u32::try_from(index).ok()?, intrinsic))
    }
}

fn paths_match(left: &std::path::Path, right: &std::path::Path) -> bool {
    left.canonicalize().unwrap_or_else(|_| left.to_path_buf())
        == right.canonicalize().unwrap_or_else(|_| right.to_path_buf())
}
