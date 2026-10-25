//! Sole generation-safe analysis-to-codegen boundary.

use std::{collections::HashSet, sync::Arc};

use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};
use beskid_queries::{AstNodeKey, Db, IndexedNodeKind, SourceUnitId, TypedProgram, node_kind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeCompilerOperation {
    FiberEntryAddress,
    ReturnTrampolineAddress,
    PollEntryInvoke,
    /// `composition_hook_invoke(function, arguments, count)` in the canonical composition
    /// runtime: an indirect call of a compiled host-hook function with `count` (at most
    /// [`beskid_isle::COMPOSITION_HOOK_ARGUMENTS_MAX`]) pointer arguments read from `arguments`.
    CompositionHookInvoke,
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
    /// The plan does not bind to the syntax this input lowers. `check` names the failed check;
    /// `declaration` is the first declaration that does not occur exactly once, when the check
    /// is a binding check. [`CodegenInputError::into_report`] renders it as internal error E2105
    /// at that declaration.
    #[error("composition plan disagrees with the validated snapshot: {check}{}", declaration.as_ref().map(|declaration| format!(" ({declaration})")).unwrap_or_default())]
    InvalidCompositionPlan { check: &'static str, declaration: Option<UnboundCompositionDeclaration> },
}

/// A composition declaration the plan names by unit, node kind and span that the lowered syntax
/// does not hold exactly once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnboundCompositionDeclaration {
    pub unit: std::path::PathBuf,
    pub kind: IndexedNodeKind,
    pub span: beskid_analysis::syntax::SpanInfo,
    /// How many nodes of `kind` the lowered syntax has at `span` (0, or more than one).
    pub found: usize,
}

impl std::fmt::Display for UnboundCompositionDeclaration {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{:?} at {}:{}:{} matches {} nodes, expected 1",
            self.kind,
            self.unit.display(),
            self.span.line_col_start.0,
            self.span.line_col_start.1,
            self.found
        )
    }
}

impl CodegenInputError {
    /// Convert into the `anyhow` error a host reports, `"{context}: {self}"`. An unbindable
    /// composition plan also carries internal error E2105 as a structured diagnostic at the
    /// declaration that did not bind (at the start of the entry unit when the failed check names
    /// no declaration), so it renders with its code and source excerpt.
    pub fn into_report(self, assembly: &beskid_analysis::projects::ProgramAssembly, context: &str) -> anyhow::Error {
        let message = format!("{context}: {self}");
        let Self::InvalidCompositionPlan { check, declaration } = self else {
            return anyhow::anyhow!(message);
        };
        let reason = match &declaration {
            Some(declaration) => format!(
                "the composition plan does not bind to the lowered syntax ({check}): {} {:?} nodes at this \
                 declaration, expected exactly 1",
                declaration.found, declaration.kind
            ),
            None => format!("the composition plan disagrees with the validated composition ({check})"),
        };
        let kind = beskid_analysis::analysis::SemanticIssueKind::InternalBuildFailure { reason };
        let located = declaration.as_ref().and_then(|declaration| {
            let unit = assembly.units.iter().find(|unit| {
                unit.path == declaration.unit
                    || std::fs::canonicalize(&unit.path)
                        .ok()
                        .zip(std::fs::canonicalize(&declaration.unit).ok())
                        .is_some_and(|(a, b)| a == b)
            })?;
            let mut diagnostic = beskid_analysis::analysis::diagnostic_kinds::issue_diagnostic(
                &unit.logical_name,
                &unit.source,
                declaration.span,
                &kind,
            );
            diagnostic.label = "this declaration does not bind".to_owned();
            Some(diagnostic)
        });
        let diagnostic = located.or_else(|| {
            let unit = assembly.units.get(assembly.entry_index)?;
            let start =
                beskid_analysis::syntax::SpanInfo { start: 0, end: 0, line_col_start: (1, 1), line_col_end: (1, 1) };
            let mut diagnostic =
                beskid_analysis::analysis::diagnostic_kinds::issue_diagnostic(&unit.logical_name, &unit.source, start, &kind);
            diagnostic.label = "while compiling this program".to_owned();
            Some(diagnostic)
        });
        match diagnostic {
            Some(diagnostic) => {
                anyhow::Error::new(beskid_analysis::services::SemanticDiagnosticsError::from_diagnostics(vec![diagnostic]))
                    .context(message)
            }
            None => anyhow::anyhow!(message),
        }
    }
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
    native_mod_callbacks: Option<crate::native_mod::callbacks::NativeModCallbackCapability>,
    glue_imports: Arc<std::collections::HashMap<AstNodeKey, String>>,
    glue_handle_bindings: Arc<[AstNodeKey]>,
}

/// A [`CodegenInput`] without its database handle ([`CodegenInput::detach`]).
pub(crate) struct DetachedCodegenInput {
    typed_program: TypedProgram,
    roots: Arc<[AstNodeKey]>,
    target: TargetMetadata,
    abi_manifest: AbiManifestV5,
    artifact_namespace: Arc<str>,
    composition_plan: Option<Arc<beskid_analysis::composition::BindingPlan>>,
    composition_snapshot: Option<Arc<beskid_analysis::composition::CompositionSnapshot>>,
    native_mod_callbacks: Option<crate::native_mod::callbacks::NativeModCallbackCapability>,
    glue_imports: Arc<std::collections::HashMap<AstNodeKey, String>>,
    glue_handle_bindings: Arc<[AstNodeKey]>,
}

impl DetachedCodegenInput {
    /// Re-attach to a handle of the storage the input was validated against.
    pub(crate) fn attach(self, db: &dyn Db) -> CodegenInput<'_> {
        CodegenInput {
            db,
            typed_program: self.typed_program,
            roots: self.roots,
            target: self.target,
            abi_manifest: self.abi_manifest,
            artifact_namespace: self.artifact_namespace,
            composition_plan: self.composition_plan,
            composition_snapshot: self.composition_snapshot,
            native_mod_callbacks: self.native_mod_callbacks,
            glue_imports: self.glue_imports,
            glue_handle_bindings: self.glue_handle_bindings,
        }
    }
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

        let entry_matches = typed_program
            .assembly
            .units
            .iter()
            .any(|unit| SourceUnitId::new(db, unit.path.clone()) == typed_program.entry);
        if !entry_matches {
            return Err(CodegenInputError::InvalidEntry);
        }

        for root in roots.iter().copied() {
            let belongs_to_assembly =
                typed_program.assembly.units.iter().any(|unit| SourceUnitId::new(db, unit.path.clone()) == root.unit);
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
            native_mod_callbacks: None,
            glue_imports: Arc::new(std::collections::HashMap::new()),
            glue_handle_bindings: Arc::from([]),
        })
    }

    pub fn database(&self) -> &'db dyn Db {
        self.db
    }

    pub fn typed_program(&self) -> &TypedProgram {
        &self.typed_program
    }

    /// Location-independent label of one assembly unit, for symbol names and source identities.
    ///
    /// Workspace materialization places the project's own sources and every dependency (Corelib
    /// packages included) beneath the compiled project's root, under directory names derived from
    /// portable lock identities. The label is the unit path relative to that root, so two copies of
    /// the same project at different locations produce equal symbols and identities. A unit outside
    /// the root (a compiler-embedded corpus) keeps its logical name, which is already relative.
    pub fn portable_unit_label(&self, path: &std::path::Path, logical_name: &str) -> String {
        let root = self.typed_program.project.project_root(self.db);
        let relative = path.strip_prefix(&root).ok().map(std::path::Path::to_path_buf).or_else(|| {
            let canonical = root.canonicalize().ok()?;
            path.strip_prefix(canonical).ok().map(std::path::Path::to_path_buf)
        });
        match relative {
            Some(relative) => relative
                .components()
                .map(|component| component.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/"),
            None => logical_name.to_owned(),
        }
    }

    /// Run-independent tag of a syntax generation for emitted names and identities: its distance
    /// from this program's generation (`0` for the program's own generation, `m<n>` for an older
    /// one). A restored session snapshot resumes generation numbering, so absolute numbers differ
    /// between otherwise identical compilations while the distance does not. Distinct generations
    /// keep distinct tags, so names stay unique.
    pub fn generation_tag(&self, generation: beskid_queries::SyntaxGenerationId) -> String {
        let base = self.typed_program.generation.0;
        if generation.0 >= base { (generation.0 - base).to_string() } else { format!("m{}", base - generation.0) }
    }

    /// Link-name fragment of [`Self::portable_unit_label`]: every byte outside `[A-Za-z0-9]` is `_`.
    pub fn portable_unit_symbol_label(&self, path: &std::path::Path, logical_name: &str) -> String {
        self.portable_unit_label(path, logical_name)
            .chars()
            .map(|character| if character.is_ascii_alphanumeric() { character } else { '_' })
            .collect()
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
            native_mod_callbacks: self.native_mod_callbacks.clone(),
            glue_imports: self.glue_imports.clone(),
            glue_handle_bindings: self.glue_handle_bindings.clone(),
        }
    }

    /// Every field of this validated input except its database handle, so the input can be
    /// re-attached to another handle of the same Salsa storage on a worker thread (see
    /// [`beskid_queries::Db::fork_db`]). All handles observe the one revision this input was
    /// validated against, because nothing writes while emission runs.
    pub(crate) fn detach(&self) -> DetachedCodegenInput {
        DetachedCodegenInput {
            typed_program: self.typed_program.clone(),
            roots: self.roots.clone(),
            target: self.target.clone(),
            abi_manifest: self.abi_manifest.clone(),
            artifact_namespace: self.artifact_namespace.clone(),
            composition_plan: self.composition_plan.clone(),
            composition_snapshot: self.composition_snapshot.clone(),
            native_mod_callbacks: self.native_mod_callbacks.clone(),
            glue_imports: self.glue_imports.clone(),
            glue_handle_bindings: self.glue_handle_bindings.clone(),
        }
    }

    pub(crate) fn with_native_mod_callbacks(
        mut self,
        capability: crate::native_mod::callbacks::NativeModCallbackCapability,
    ) -> Self {
        self.native_mod_callbacks = Some(capability);
        self
    }
    /// Issued only by current Glue packet/native-body preparation. Ordinary
    /// C ABI callers and runtime source never inherit normalized Rust transport.
    pub(crate) fn with_glue_imports(&self, imports: std::collections::HashMap<AstNodeKey, String>) -> Self {
        let mut input = self.with_artifact_namespace(self.artifact_namespace.clone());
        input.glue_imports = Arc::new(imports);
        input
    }
    pub(crate) fn with_glue_handle_bindings(&self, bindings: Arc<[AstNodeKey]>) -> Self {
        let mut input = self.with_artifact_namespace(self.artifact_namespace.clone());
        input.glue_handle_bindings = bindings;
        input
    }
    pub(crate) fn glue_handle_bindings(&self) -> &[AstNodeKey] {
        &self.glue_handle_bindings
    }
    pub(crate) fn glue_import_symbol(&self, declaration: AstNodeKey) -> Option<&str> {
        self.glue_imports.get(&declaration).map(String::as_str)
    }
    pub(crate) fn glue_import_callee_symbol(&self, callee: &beskid_isle::DirectCallee) -> Option<&str> {
        self.glue_imports
            .iter()
            .find_map(|(key, symbol)| (*callee == beskid_isle::DirectCallee::item(*key)).then_some(symbol.as_str()))
    }
    pub(crate) fn permits_native_mod_callback(&self, callback: beskid_queries::NativeModCallback) -> bool {
        self.native_mod_callbacks.as_ref().is_some_and(|capability| capability.authorizes(callback))
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
        if !snapshot
            .source_unit_path
            .as_deref()
            .is_some_and(|path| SourceUnitId::new(self.db, path.to_path_buf()) == self.typed_program.entry)
        {
            return Err(CodegenInputError::ForeignCompositionUnit);
        }
        // The plan names every declaration by unit, kind and span. Node ids are pre-order
        // positions in the syntax this input lowers, which typing extends (filled optional
        // arguments), so they are bound here, against exactly that syntax, and nowhere else.
        let mut plan = Arc::unwrap_or_clone(plan);
        let mut snapshot = Arc::unwrap_or_clone(snapshot);
        let (db, entry) = (self.db, self.typed_program.entry);
        // The first declaration that does not bind, per check, for the E2105 report.
        let bind = |unit: &beskid_analysis::composition::UnitPath,
                    kind: IndexedNodeKind,
                    span: beskid_analysis::syntax::SpanInfo|
         -> Result<beskid_queries::AstNodeId, UnboundCompositionDeclaration> {
            let unit = match unit {
                None => entry,
                Some(path) => SourceUnitId::new(db, path.to_path_buf()),
            };
            beskid_queries::node_at_span(db, unit, generation, kind, span).map(|key| key.node).map_err(|found| {
                UnboundCompositionDeclaration { unit: unit.path(db).to_path_buf(), kind, span, found }
            })
        };
        let mut unbound_registration_source = None;
        for registration in &mut snapshot.registrations {
            match bind(&registration.unit, IndexedNodeKind::RegistryEntry, registration.span) {
                Ok(node) => registration.source_node_id = node,
                Err(declaration) => {
                    unbound_registration_source.get_or_insert(declaration);
                }
            }
        }
        let mut unbound_singular = None;
        for singular in &mut plan.singulars {
            match bind(&singular.field_unit, IndexedNodeKind::Field, singular.field_span) {
                Ok(node) => singular.field_node_id = node,
                Err(declaration) => {
                    unbound_singular.get_or_insert(declaration);
                }
            }
        }
        let mut unbound_plural = None;
        for plural in &mut plan.plurals {
            match bind(&plural.field_unit, IndexedNodeKind::Field, plural.field_span) {
                Ok(node) => plural.field_node_id = node,
                Err(declaration) => {
                    unbound_plural.get_or_insert(declaration);
                }
            }
        }
        let mut unbound_hook = None;
        for hook in plan.init_hooks.iter_mut().chain(&mut plan.startup_hooks).chain(&mut plan.disposal_hooks) {
            match (
                bind(&hook.unit, IndexedNodeKind::ScopeHook, hook.span),
                bind(&hook.unit, IndexedNodeKind::FunctionDefinition, hook.span),
            ) {
                (Ok(source), Ok(function)) => {
                    hook.source_node_id = source;
                    hook.function_node_id = function;
                }
                (Err(declaration), _) | (_, Err(declaration)) => {
                    unbound_hook.get_or_insert(declaration);
                }
            }
        }
        let (bound_registration_sources, bound_singulars, bound_plurals, bound_hooks) = (
            unbound_registration_source.is_none(),
            unbound_singular.is_none(),
            unbound_plural.is_none(),
            unbound_hook.is_none(),
        );

        let registrations = snapshot.registrations.iter().map(|registration| registration.id).collect::<HashSet<_>>();
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
        let valid_plurals = bound_plurals
            && plan.plurals.iter().all(|plural| {
                registrations.contains(&plural.owner_registration_id)
                    && plural.target_slots.iter().all(|slot| assigned_slots.contains(&slot.0))
            });
        let valid_singulars = bound_singulars
            && plan.singulars.iter().all(|singular| {
                registrations.contains(&singular.owner_registration_id)
                    && assigned_slots.contains(&singular.target_slot.0)
            });
        let valid_scopes = snapshot.scope_names.keys().all(|scope_id| plan.scope_parents.contains_key(scope_id));
        let valid_hooks = bound_hooks
            && plan.init_hooks.iter().chain(&plan.startup_hooks).chain(&plan.disposal_hooks).all(|hook| {
                (hook.scope_id == beskid_analysis::composition::ScopeId::GLOBAL
                    || plan.scope_parents.contains_key(&hook.scope_id))
                    && hook.arguments_registration_id.is_none_or(|id| registrations.contains(&id))
            });
        let defect = [
            (plan.launched_host == snapshot.launched_host, "launched host", None),
            (valid_activation, "activation slots", None),
            (bound_registration_sources, "registration source nodes", unbound_registration_source),
            (valid_singulars, "singular injection fields", unbound_singular),
            (valid_plurals, "plural injection fields", unbound_plural),
            (valid_scopes, "scope parents", None),
            (valid_hooks, "hook nodes", unbound_hook),
        ]
        .into_iter()
        .find_map(|(valid, check, declaration)| (!valid).then_some((check, declaration)));
        if let Some((check, declaration)) = defect {
            return Err(CodegenInputError::InvalidCompositionPlan { check, declaration });
        }
        let (plan, snapshot) = (Arc::new(plan), Arc::new(snapshot));
        self.composition_plan = Some(plan);
        self.composition_snapshot = Some(snapshot);
        Ok(self)
    }

    /// The exact source key of a composition declaration in `unit` (`None`: the entry unit).
    pub fn composition_unit_key(
        &self,
        unit: &beskid_analysis::composition::UnitPath,
        node: beskid_queries::AstNodeId,
    ) -> AstNodeKey {
        AstNodeKey {
            unit: match unit {
                None => self.typed_program.entry,
                Some(path) => SourceUnitId::new(self.db, path.to_path_buf()),
            },
            generation: self.typed_program.generation,
            node,
        }
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
    pub fn runtime_compiler_operation_for(&self, key: AstNodeKey, name: &str) -> Option<RuntimeCompilerOperation> {
        if !matches!(node_kind(self.db, key), Ok(Some(_))) || key.generation != self.typed_program.generation {
            return None;
        }
        let logical_path = self
            .typed_program
            .assembly
            .units
            .iter()
            .find(|unit| SourceUnitId::new(self.db, unit.path.clone()) == key.unit)?
            .logical_name
            .as_str();
        let capability = self.runtime_intrinsic_capability()?;
        capability.authorizes_source(logical_path).then_some(())?;
        match (logical_path, name) {
            (beskid_abi::runtime_source::CANONICAL_SCHEDULER_CONTEXT_SOURCE_PATH, "scheduler_fiber_entry_address") => {
                Some(RuntimeCompilerOperation::FiberEntryAddress)
            }
            (
                beskid_abi::runtime_source::CANONICAL_SCHEDULER_CONTEXT_SOURCE_PATH,
                "scheduler_return_trampoline_address",
            ) => Some(RuntimeCompilerOperation::ReturnTrampolineAddress),
            (beskid_abi::runtime_source::CANONICAL_SCHEDULER_POLL_SOURCE_PATH, "scheduler_poll_entry_invoke") => {
                Some(RuntimeCompilerOperation::PollEntryInvoke)
            }
            (beskid_abi::runtime_source::CANONICAL_COMPOSITION_SOURCE_PATH, "composition_hook_invoke") => {
                Some(RuntimeCompilerOperation::CompositionHookInvoke)
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
        let logical_path = self
            .typed_program
            .assembly
            .units
            .iter()
            .find(|unit| SourceUnitId::new(self.db, unit.path.clone()) == key.unit)?
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
