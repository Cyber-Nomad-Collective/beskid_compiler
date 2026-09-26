//! Sole generation-safe analysis-to-codegen boundary.

use std::{collections::HashSet, sync::Arc};

use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};
use beskid_queries::{AstNodeKey, Db, IndexedNodeKind, TypedProgram, node_kind, node_span};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulerCompilerOperation {
    FiberEntryAddress,
    ReturnTrampolineAddress,
    PollEntryInvoke,
}

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
    #[error("composition plan belongs to a different syntax generation")]
    StaleCompositionPlan,
    #[error("composition snapshot belongs to a foreign source unit")]
    ForeignCompositionUnit,
    #[error("composition plan disagrees with the validated snapshot")]
    InvalidCompositionPlan,
}

/// Complete typed input required before generated ISLE selection may begin.
pub struct CodegenInput<'db> {
    db: &'db dyn Db,
    typed_program: TypedProgram,
    roots: Arc<[AstNodeKey]>,
    target: TargetMetadata,
    abi_manifest: AbiManifestV5,
    artifact_namespace: Arc<str>,
    composition_plan: Option<Arc<beskid_analysis::composition::BindingPlan>>,
    composition_snapshot: Option<Arc<beskid_analysis::composition::CompositionSnapshot>>,
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
        target.validate().map_err(|_| CodegenInputError::InvalidTarget)?;
        abi_manifest.validate().map_err(|_| CodegenInputError::ManifestDrift)?;
        if abi_manifest.target != target {
            return Err(CodegenInputError::ManifestTargetMismatch);
        }
        if abi_manifest != AbiManifestV5::canonical_runtime(target.clone()) {
            return Err(CodegenInputError::ManifestDrift);
        }

        let entry_path = typed_program.entry.path(db);
        let entry_matches = typed_program.assembly.units.iter().any(|unit| paths_match(&unit.path, entry_path));
        if !entry_matches {
            return Err(CodegenInputError::InvalidEntry);
        }

        for root in roots.iter().copied() {
            let unit_path = root.unit.path(db);
            let belongs_to_assembly =
                typed_program.assembly.units.iter().any(|unit| paths_match(&unit.path, unit_path));
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
            artifact_namespace: Arc::from("module"),
            composition_plan: None,
            composition_snapshot: None,
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

    /// Bind all source-owned static artifacts to one caller-selected module emission session.
    /// The namespace is not a language symbol and is only used to prevent collisions when a
    /// long-lived Cranelift module receives more than one source artifact.
    pub fn with_artifact_namespace(&self, artifact_namespace: Arc<str>) -> Self {
        Self {
            db: self.db,
            typed_program: self.typed_program.clone(),
            roots: self.roots.clone(),
            target: self.target.clone(),
            abi_manifest: self.abi_manifest.clone(),
            artifact_namespace,
            composition_plan: self.composition_plan.clone(),
            composition_snapshot: self.composition_snapshot.clone(),
        }
    }

    pub fn artifact_namespace(&self) -> &str {
        &self.artifact_namespace
    }

    /// Attach compiler-resolved composition facts for this exact syntax generation.
    ///
    /// Ordinary dynamic lookup has no fallback: composition lowering is authorized only when
    /// this generation-bound plan is present.
    pub fn with_composition_authority(
        mut self,
        generation: beskid_queries::SyntaxGenerationId,
        plan: Arc<beskid_analysis::composition::BindingPlan>,
        snapshot: Arc<beskid_analysis::composition::CompositionSnapshot>,
    ) -> Result<Self, CodegenInputError> {
        if generation != self.typed_program.generation {
            return Err(CodegenInputError::StaleCompositionPlan);
        }
        if snapshot.source_unit_path.as_ref() != Some(&self.typed_program.assembly.entry_unit().path) {
            return Err(CodegenInputError::ForeignCompositionUnit);
        }
        let registrations = snapshot.registrations.iter().map(|registration| registration.id).collect::<HashSet<_>>();
        let source_key = |node| AstNodeKey { unit: self.typed_program.entry, generation, node };
        let valid_registration_sources = snapshot.registrations.iter().all(|registration| {
            let key = source_key(registration.source_node_id);
            registration.source_node_id.is_valid()
                && matches!(node_kind(self.db, key), Ok(Some(IndexedNodeKind::RegistryEntry)))
                && matches!(node_span(self.db, key), Ok(Some(span)) if span == registration.span)
        });
        let mut assigned_registrations = HashSet::new();
        let mut assigned_slots = HashSet::new();
        let valid_activation = plan.activation.len() == snapshot.registrations.len()
            && registrations.len() == snapshot.registrations.len()
            && plan.activation.iter().all(|entry| {
                registrations.contains(&entry.registration_id)
                    && assigned_registrations.insert(entry.registration_id)
                    && usize::try_from(entry.slot.0).is_ok_and(|slot| slot < plan.activation.len())
                    && assigned_slots.insert(entry.slot.0)
            });
        let valid_plurals = plan.plurals.iter().all(|plural| {
            registrations.contains(&plural.owner_registration_id)
                && plural.target_slots.iter().all(|slot| assigned_slots.contains(&slot.0))
                && plural.field_node_id.is_valid()
                && matches!(node_kind(self.db, source_key(plural.field_node_id)), Ok(Some(IndexedNodeKind::Field)))
                && matches!(node_span(self.db, source_key(plural.field_node_id)), Ok(Some(span)) if span == plural.field_span)
        });
        let valid_singulars = plan.singulars.iter().all(|singular| {
            registrations.contains(&singular.owner_registration_id)
                && assigned_slots.contains(&singular.target_slot.0)
                && singular.field_node_id.is_valid()
                && matches!(node_kind(self.db, source_key(singular.field_node_id)), Ok(Some(IndexedNodeKind::Field)))
                && matches!(node_span(self.db, source_key(singular.field_node_id)), Ok(Some(span)) if span == singular.field_span)
        });
        let valid_scopes = snapshot.scope_names.keys().all(|scope_id| plan.scope_parents.contains_key(scope_id));
        let valid_hooks = plan.init_hooks.iter().chain(&plan.startup_hooks).chain(&plan.disposal_hooks).all(|hook| {
            (hook.scope_id == beskid_analysis::composition::ScopeId::GLOBAL
                || plan.scope_parents.contains_key(&hook.scope_id))
                && hook.source_node_id.is_valid()
                && matches!(
                    node_kind(
                        self.db,
                        AstNodeKey { unit: self.typed_program.entry, generation, node: hook.source_node_id }
                    ),
                    Ok(Some(IndexedNodeKind::ScopeHook))
                )
        });
        if plan.launched_host != snapshot.launched_host
            || !valid_activation
            || !valid_registration_sources
            || !valid_singulars
            || !valid_plurals
            || !valid_scopes
            || !valid_hooks
        {
            return Err(CodegenInputError::InvalidCompositionPlan);
        }
        self.composition_plan = Some(plan);
        self.composition_snapshot = Some(snapshot);
        Ok(self)
    }

    pub fn composition_authority(
        &self,
    ) -> Option<(&beskid_analysis::composition::BindingPlan, &beskid_analysis::composition::CompositionSnapshot)> {
        Some((self.composition_plan.as_deref()?, self.composition_snapshot.as_deref()?))
    }

    /// The one context layout selected by the ABI-v5 target contract.
    ///
    /// Context storage is never inferred from a scheduler-record offset: the
    /// compiler materializes this exact manifest record for canonical runtime
    /// calls to `arch_context_size` and `arch_context_alignment`.
    pub fn target_context_layout(&self) -> Option<&beskid_abi::abi_v5::AbiLayout> {
        let name = match self.target.triple.as_str() {
            "x86_64-unknown-linux-gnu" => "BeskidArchContextX86_64SysV",
            "aarch64-apple-darwin" => "BeskidArchContextAarch64Darwin",
            "x86_64-pc-windows-msvc" => "BeskidArchContextX86_64Windows",
            _ => return None,
        };
        self.abi_manifest.layouts.iter().find(|layout| layout.name == name)
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
    /// Resolve an operation minted by the compiler for the exact embedded Scheduler source.
    ///
    /// These operations are local lowering artifacts, not manifest imports. The source unit,
    /// corpus capability, syntax generation, and operation spelling must all match exactly.
    pub fn scheduler_compiler_operation_for(&self, key: AstNodeKey, name: &str) -> Option<SchedulerCompilerOperation> {
        if !matches!(node_kind(self.db, key), Ok(Some(_))) || key.generation != self.typed_program.generation {
            return None;
        }
        let unit_path = key.unit.path(self.db);
        let logical_path = self
            .typed_program
            .assembly
            .units
            .iter()
            .find(|unit| paths_match(&unit.path, unit_path))?
            .logical_name
            .as_str();
        let capability = self.runtime_intrinsic_capability()?;
        capability.authorizes_source(logical_path).then_some(())?;
        match (logical_path, name) {
            (beskid_abi::runtime_source::CANONICAL_SCHEDULER_CONTEXT_SOURCE_PATH, "scheduler_fiber_entry_address") => {
                Some(SchedulerCompilerOperation::FiberEntryAddress)
            }
            (
                beskid_abi::runtime_source::CANONICAL_SCHEDULER_CONTEXT_SOURCE_PATH,
                "scheduler_return_trampoline_address",
            ) => Some(SchedulerCompilerOperation::ReturnTrampolineAddress),
            (beskid_abi::runtime_source::CANONICAL_SCHEDULER_POLL_SOURCE_PATH, "scheduler_poll_entry_invoke") => {
                Some(SchedulerCompilerOperation::PollEntryInvoke)
            }
            _ => None,
        }
    }

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
            .units
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
