//! Host-neutral prepared-syntax entrypoint lowering.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use anyhow::Result;
use beskid_abi::{
    abi_v5::{AbiManifestV5, TargetMetadata},
    runtime_provenance::RuntimeProvenanceAudit,
    runtime_source::{
        CANONICAL_BOOTSTRAP_SOURCE_PATH, canonical_corelib_syscall_service_capability,
        canonical_runtime_intrinsic_capability, canonical_runtime_sources,
    },
};
use beskid_analysis::{
    projects::{AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit},
    services::{FrontEndTypedResult, parse_program_with_source_name},
};
use beskid_isle::AstNodeKey;
use beskid_queries::{
    BeskidDatabase, SemanticTypeId, SourceUnitId, SyntaxGenerationId, build_canonical_runtime_typed_program,
    build_typed_program_with_corelib_syscall_services, child_nodes, format_ast_node_trace, item_abi_signature,
    item_body, item_export_symbol, item_name, node_kind, project_session_for_syntax_assembly, reachable_items,
};
use cranelift_codegen::isa::TargetIsa;

use crate::{CodegenArtifact, CodegenInput, ExportEntry, SyntaxModuleItem, lower_syntax_program};

/// Result of lowering one prepared syntax entrypoint through the typed-codegen boundary.
pub struct PreparedSyntaxEntrypoint {
    artifact: CodegenArtifact,
    symbol: String,
    return_type: SemanticTypeId,
}
impl PreparedSyntaxEntrypoint {
    pub fn artifact(&self) -> &CodegenArtifact {
        &self.artifact
    }
    pub fn symbol(&self) -> &str {
        &self.symbol
    }
    pub fn return_type(&self) -> SemanticTypeId {
        self.return_type
    }
    pub fn into_artifact(self) -> CodegenArtifact {
        self.artifact
    }
}

/// One checked no-argument entry selected from a prepared generation.
pub struct PreparedSyntaxEntry {
    name: String,
    symbol: String,
    return_type: SemanticTypeId,
}
impl PreparedSyntaxEntry {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn symbol(&self) -> &str {
        &self.symbol
    }
    pub fn return_type(&self) -> SemanticTypeId {
        self.return_type
    }
}

/// A single emitted reachability union with entries in the caller's selection order.
/// Its selected entries and emitted body are one compiler-issued packet.
///
/// ```compile_fail
/// fn substitute(prepared: &mut beskid_codegen::PreparedSyntaxEntrypoints) {
///     prepared.artifact = Default::default();
/// }
/// ```
pub struct PreparedSyntaxEntrypoints {
    artifact: CodegenArtifact,
    entries: Vec<PreparedSyntaxEntry>,
}
impl PreparedSyntaxEntrypoints {
    pub fn artifact(&self) -> &CodegenArtifact {
        &self.artifact
    }
    pub fn entries(&self) -> &[PreparedSyntaxEntry] {
        &self.entries
    }
    pub fn into_artifact(self) -> CodegenArtifact {
        self.artifact
    }
}

/// Callable metadata issued from the same registered items that produced an artifact.
/// Physical pointer signatures do not authorize native SDK request marshaling.
#[derive(Debug, Clone)]
pub struct PreparedSyntaxCallable {
    key: AstNodeKey,
    internal_symbol: String,
    link_symbol: String,
    signature: beskid_queries::ItemSignature,
}

impl PreparedSyntaxCallable {
    pub fn key(&self) -> AstNodeKey {
        self.key
    }
    pub fn internal_symbol(&self) -> &str {
        &self.internal_symbol
    }
    pub fn link_symbol(&self) -> &str {
        &self.link_symbol
    }
    pub fn signature(&self) -> &beskid_queries::ItemSignature {
        &self.signature
    }
}

/// A prepared module and its exact executable source declarations.
/// Generic specializations discovered while lowering remain artifact-owned; a bare generic
/// declaration cannot acquire a callable without an exact instantiated signature.
pub struct PreparedSyntaxModule {
    artifact: CodegenArtifact,
    target: TargetMetadata,
    callables: Vec<PreparedSyntaxCallable>,
}

impl PreparedSyntaxModule {
    pub fn artifact(&self) -> &CodegenArtifact {
        &self.artifact
    }
    pub fn target(&self) -> &TargetMetadata {
        &self.target
    }
    pub fn callables(&self) -> &[PreparedSyntaxCallable] {
        &self.callables
    }
    pub fn callable(&self, key: AstNodeKey) -> Option<&PreparedSyntaxCallable> {
        self.callables.iter().find(|callable| callable.key == key)
    }
    pub fn into_artifact(self) -> CodegenArtifact {
        self.artifact
    }
}

/// Lower selected entries once through their shared generation-bound authority.
pub fn lower_prepared_syntax_entrypoints(
    db: &mut BeskidDatabase,
    front: &FrontEndTypedResult,
    entrypoints: &[String],
    target: TargetMetadata,
    isa: &dyn TargetIsa,
) -> Result<PreparedSyntaxEntrypoints> {
    lower_syntax_assembly_entrypoints_with_composition(
        db,
        Arc::new(front.syntax_assembly()),
        entrypoints,
        target,
        isa,
        Some(front),
    )
}

/// Lower the compiler-embedded canonical runtime corpus through prepared syntax and ISLE.
/// Only this constructor can mint the matching intrinsic capability; a runtime kit cannot be
/// built from a caller-supplied source file or host shim.
pub fn lower_canonical_runtime_prepared_syntax(
    db: &mut BeskidDatabase,
    target: TargetMetadata,
    isa: &dyn TargetIsa,
) -> Result<CodegenArtifact> {
    let sources = canonical_runtime_sources();
    let bootstrap = sources
        .iter()
        .find(|unit| unit.logical_path == CANONICAL_BOOTSTRAP_SOURCE_PATH)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("canonical Bootstrap source is missing"))?;
    let root_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime/beskid");
    let bootstrap_path = root_dir.join(&bootstrap.logical_path);
    let units = sources
        .into_iter()
        .map(|source| {
            let path = root_dir.join(&source.logical_path);
            let program =
                parse_program_with_source_name(path.to_str().unwrap_or_default(), &source.source).map_err(|error| {
                    anyhow::anyhow!("canonical runtime parse failed for {}: {error}", source.logical_path)
                })?;
            Ok(SourceUnit {
                logical_name: source.logical_path,
                origin_path: path.clone(),
                path,
                source: source.source,
                program,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let bootstrap_index = units
        .iter()
        .position(|unit| unit.path == bootstrap_path)
        .ok_or_else(|| anyhow::anyhow!("parsed canonical Bootstrap source is missing"))?;
    let generation = SyntaxGenerationId(1);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root_dir.join("src") },
            dependencies: vec![RootEntry { dependency_name: Some("canonical-corelib".into()), source_root: root_dir }],
        },
        Arc::new(units),
        bootstrap_index,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let project = project_session_for_syntax_assembly(db, &assembly, "beskid-runtime-native", "canonical-runtime")
        .map_err(|error| anyhow::anyhow!("canonical runtime session preparation failed: {error}"))?;
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let capability = canonical_runtime_intrinsic_capability(&manifest)
        .map_err(|error| anyhow::anyhow!("canonical runtime intrinsic capability unavailable: {error:?}"))?;
    let typed = build_canonical_runtime_typed_program(db, project, generation, assembly.clone(), capability)
        .map_err(|error| anyhow::anyhow!("canonical runtime syntax preparation failed: {error}"))?;
    // Runtime ABI exports may be implemented by their owning canonical module rather than
    // Bootstrap. Select every manifest-declared export across the embedded runtime units, then
    // follow only direct calls from those entries. Lowering every function declaration would try
    // to instantiate generic helpers that have no call-derived ABI specialization.
    let roots = assembly
        .units
        .iter()
        .map(|unit| AstNodeKey {
            unit: SourceUnitId::new(db, unit.path.clone()),
            generation,
            node: beskid_queries::AstNodeId(0),
        })
        .collect::<Vec<_>>();
    let input = CodegenInput::new(db, typed, Arc::from(roots), target.clone(), manifest).map_err(|error| {
        let context = match &error {
            crate::codegen_input::CodegenInputError::InvalidRoot(root) => {
                let path = root.unit.path(db);
                let registered = db.syntax_unit(root.unit).map(|unit| unit.accepts_key(db, *root));
                format!(
                    " path={} exists={} registered_current={registered:?} kind={:?}",
                    path.display(),
                    path.exists(),
                    node_kind(db, *root)
                )
            }
            _ => String::new(),
        };
        anyhow::anyhow!("canonical runtime CodegenInput failed: {error}{context}")
    })?;
    // The provenance audit is the single canonical registration surface for symbols that may be
    // defined by the native runtime image. Intersecting it with explicit source `[Export]`
    // declarations includes both public ABI entries and runtime-owned Corelib service adapters,
    // while platform-only symbols remain owned by the platform object builders.
    let runtime_source_exports = RuntimeProvenanceAudit::canonical(target.clone())
        .map_err(|error| anyhow::anyhow!("canonical runtime export policy is unavailable: {error}"))?
        .allowed_defined_symbols
        .into_iter()
        .collect::<HashSet<_>>();
    let mut exported_items = HashMap::new();
    for key in input.roots().iter().copied().flat_map(|root| function_definitions(input.database(), root)) {
        let export = item_export_symbol(input.database(), key)
            .map_err(|error| anyhow::anyhow!("canonical runtime export validation failed: {error}"))?;
        if let Some(export) = export
            && runtime_source_exports.contains(&*export.0)
            && exported_items.insert(export.0.to_string(), key).is_some()
        {
            anyhow::bail!("canonical runtime declares duplicate ABI export `{}`", export.0);
        }
    }
    let mut items = Vec::new();
    let mut selected = HashSet::new();
    let mut entry_roots = exported_items.iter().map(|(name, key)| (name.clone(), *key)).collect::<Vec<_>>();
    for key in input.roots().iter().copied().flat_map(|root| function_definitions(input.database(), root)) {
        if let Some(name) = item_name(input.database(), key)?
            && crate::module_emission::SCHEDULER_ENTRY_HELPERS.contains(&name.as_ref())
        {
            entry_roots.push((name.to_string(), key));
        }
    }
    for (export, entry) in &entry_roots {
        let entry = *entry;
        let program = input
            .roots()
            .iter()
            .copied()
            .find(|root| root.unit == entry.unit)
            .ok_or_else(|| anyhow::anyhow!("canonical runtime export `{export}` has no source root"))?;
        let reachable = reachable_items(input.database(), program, entry)
            .map_err(|error| anyhow::anyhow!("canonical runtime reachability failed for `{export}`: {error}"))?
            .ok_or_else(|| anyhow::anyhow!("incomplete direct-call facts for canonical runtime export `{export}`"))?;
        for key in reachable.iter().copied() {
            if !selected.insert(key) {
                continue;
            }
            let symbol = item_export_symbol(input.database(), key)
                .map_err(|error| anyhow::anyhow!("canonical runtime export validation failed: {error}"))?
                .filter(|symbol| runtime_source_exports.contains(&*symbol.0))
                .map(|symbol| symbol.0.to_string())
                .or_else(|| syntax_item_symbol(input.database(), &input, key))
                .ok_or_else(|| anyhow::anyhow!("canonical runtime reachable item has no syntax symbol"))?;
            items.push(SyntaxModuleItem { key, symbol });
        }
    }
    if items.is_empty() {
        anyhow::bail!("canonical runtime source corpus has no declared exports");
    }
    let mut artifact = lower_syntax_program(&input, isa, &items)
        .map_err(|error| error.into_report(&input, "canonical runtime ISLE lowering failed"))?;
    let checked_exports = std::mem::take(&mut artifact.exports);
    artifact.exports = syntax_export_entries_matching(input.database(), &items, &runtime_source_exports)?;
    for export in checked_exports {
        if beskid_abi::runtime_source::checked_runtime_clone(&export.exported_symbol).is_none()
            || !runtime_source_exports.contains(&export.exported_symbol)
        {
            anyhow::bail!("canonical runtime checked constructor export lacks manifest provenance");
        }
        if artifact.exports.iter().any(|existing| existing.exported_symbol == export.exported_symbol) {
            anyhow::bail!("canonical runtime duplicate checked constructor export");
        }
        artifact.exports.push(export);
    }
    for symbol in beskid_abi::runtime_source::CHECKED_RUNTIME_CLONES.iter().map(|clone| clone.export) {
        if !artifact.exports.iter().any(|export| export.exported_symbol == symbol) {
            anyhow::bail!("canonical runtime checked constructor `{symbol}` was not emitted");
        }
    }
    Ok(artifact)
}

/// Lower a prepared frontend snapshot with the caller's target ISA.
///
/// Hosts retain ISA selection and runtime-kit/link policy; this function owns only the shared
/// prepared-syntax → `TypedProgram` → `CodegenInput` → ISLE transition.
pub fn lower_prepared_syntax_entrypoint(
    db: &mut BeskidDatabase,
    front: &FrontEndTypedResult,
    entrypoint: &str,
    target: TargetMetadata,
    isa: &dyn TargetIsa,
) -> Result<PreparedSyntaxEntrypoint> {
    let assembly = Arc::new(front.syntax_assembly());
    lower_syntax_assembly_entrypoint_with_composition(db, assembly, entrypoint, target, isa, Some(front))
}

/// Lower one entrypoint from an assembled syntax program through ISLE.
///
/// This is the executable boundary for callers which already own a
/// [`ProgramAssembly`]. Each semantic and reachability fact is derived from the supplied
/// generation-scoped syntax assembly.
pub fn lower_syntax_assembly_entrypoint(
    db: &mut BeskidDatabase,
    assembly: Arc<ProgramAssembly>,
    entrypoint: &str,
    target: TargetMetadata,
    isa: &dyn TargetIsa,
) -> Result<PreparedSyntaxEntrypoint> {
    lower_syntax_assembly_entrypoint_with_composition(db, assembly, entrypoint, target, isa, None)
}

fn lower_syntax_assembly_entrypoint_with_composition(
    db: &mut BeskidDatabase,
    assembly: Arc<ProgramAssembly>,
    entrypoint: &str,
    target: TargetMetadata,
    isa: &dyn TargetIsa,
    front: Option<&FrontEndTypedResult>,
) -> Result<PreparedSyntaxEntrypoint> {
    let lowered =
        lower_syntax_assembly_entrypoints_with_composition(db, assembly, &[entrypoint.to_owned()], target, isa, front)?;
    let entry = lowered.entries.into_iter().next().expect("one checked selection");
    Ok(PreparedSyntaxEntrypoint { artifact: lowered.artifact, symbol: entry.symbol, return_type: entry.return_type })
}

fn lower_syntax_assembly_entrypoints_with_composition(
    db: &mut BeskidDatabase,
    assembly: Arc<ProgramAssembly>,
    entrypoints: &[String],
    target: TargetMetadata,
    isa: &dyn TargetIsa,
    front: Option<&FrontEndTypedResult>,
) -> Result<PreparedSyntaxEntrypoints> {
    if entrypoints.is_empty() {
        anyhow::bail!("at least one entrypoint must be selected");
    }
    let mut names = HashSet::new();
    for name in entrypoints {
        if name.trim().is_empty() || !names.insert(name.as_str()) {
            anyhow::bail!("entrypoint selections must be nonempty and unique: `{name}`");
        }
    }
    let entry_path = assembly.entry_unit().path.clone();
    let generation = assembly.generation;
    let project = project_session_for_syntax_assembly(db, &assembly, "syntax-codegen", "prepared-frontend")
        .map_err(|error| anyhow::anyhow!("syntax program session preparation failed: {error}"))?;
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let capability = canonical_corelib_syscall_service_capability(&manifest)
        .map_err(|error| anyhow::anyhow!("Corelib syscall service capability unavailable: {error:?}"))?;
    let typed = (if assembly.runtime_fixture.is_some() {
        beskid_queries::build_runtime_fixture_typed_program(db, project, generation, Arc::clone(&assembly), &manifest)
    } else {
        build_typed_program_with_corelib_syscall_services(db, project, generation, Arc::clone(&assembly), capability)
    })
    .map_err(|error| anyhow::anyhow!("syntax program preparation failed: {error}"))?;
    let roots = assembly
        .units
        .iter()
        .map(|unit| AstNodeKey {
            unit: SourceUnitId::new(db, unit.path.clone()),
            generation,
            node: beskid_queries::AstNodeId(0),
        })
        .collect::<Vec<_>>();
    let input = CodegenInput::new(db, typed, Arc::from(roots), target.clone(), manifest)
        .map_err(|error| anyhow::anyhow!("invalid syntax codegen input: {error}"))?;
    let input = if let Some(front) = front {
        input
            .with_composition_authority(
                generation,
                Arc::new(front.binding_plan.clone()),
                Arc::new(front.composition_snapshot.clone()),
            )
            .map_err(|error| anyhow::anyhow!("invalid composition codegen input: {error}"))?
    } else {
        input
    };
    let entry_root =
        AstNodeKey { unit: SourceUnitId::new(db, entry_path), generation, node: beskid_queries::AstNodeId(0) };
    let mut selected = HashSet::new();
    let mut items = Vec::new();
    let mut entries = Vec::with_capacity(entrypoints.len());
    for entrypoint in entrypoints {
        let entry = find_entrypoint(db, &input, entrypoint)
            .ok_or_else(|| anyhow::anyhow!("Missing entrypoint `{entrypoint}`"))?;
        let entry_label = assembly
            .units
            .iter()
            .find(|unit| SourceUnitId::new(db, unit.path.clone()) == entry.unit)
            .map(|unit| unit.logical_name.as_str())
            .unwrap_or("<unknown>");
        crate::isle_trace::event(|| {
            format!(
                "event=entry.selected entrypoint={entrypoint} site={}",
                format_ast_node_trace(db, entry, entry_label),
            )
        });
        let signature = item_abi_signature(db, entry)
            .map_err(|error| anyhow::anyhow!("entrypoint signature query failed: {error}"))?
            .ok_or_else(|| anyhow::anyhow!("Missing signature for `{entrypoint}`"))?;
        if !signature.parameters.is_empty() {
            anyhow::bail!("Entrypoint `{entrypoint}` must take no parameters");
        }
        let reachable = reachable_items(db, entry_root, entry)
            .map_err(|error| anyhow::anyhow!("entrypoint reachability query failed: {error}"))?
            .ok_or_else(|| anyhow::anyhow!("incomplete direct-call facts for `{entrypoint}`"))?;
        for key in reachable.iter().copied() {
            if selected.insert(key) {
                let symbol = syntax_item_symbol(db, &input, key)
                    .ok_or_else(|| anyhow::anyhow!("reachable item is not a syntax function or test"))?;
                items.push(SyntaxModuleItem { key, symbol });
            }
        }
        let symbol = syntax_item_symbol(db, &input, entry)
            .ok_or_else(|| anyhow::anyhow!("entrypoint `{entrypoint}` is not a syntax function or test"))?;
        entries.push(PreparedSyntaxEntry { name: entrypoint.clone(), symbol, return_type: signature.result });
    }
    close_scheduler_entry_reachability(input.database(), &input, &mut selected, &mut items)?;
    let mut artifact = lower_syntax_program(&input, isa, &items)
        .map_err(|error| error.into_report(&input, "syntax ISLE lowering failed"))?;
    artifact.exports = syntax_export_entries(db, &items)?;
    Ok(PreparedSyntaxEntrypoints { artifact, entries })
}

/// Lower every executable function and method in a prepared frontend snapshot.
///
/// Unlike [`lower_prepared_syntax_entrypoint`], this has no entrypoint convention: it emits the
/// complete prepared syntax module using only generation-scoped syntax facts. Hosts that need to
/// register methods, such as the compiler Mod command, can therefore build their artifact without
/// reconstructing legacy resolution or type-result state.
pub fn lower_prepared_syntax_module(
    db: &mut BeskidDatabase,
    front: &FrontEndTypedResult,
    target: TargetMetadata,
    isa: &dyn TargetIsa,
) -> Result<CodegenArtifact> {
    Ok(lower_prepared_syntax_module_with_callables(db, front, target, isa)?.into_artifact())
}

/// Issue executable callable identities while lowering their registered source module.
/// Consumers select by key, never by guessing a linker symbol from a method's leaf name.
pub fn lower_prepared_syntax_module_with_callables(
    db: &mut BeskidDatabase,
    front: &FrontEndTypedResult,
    target: TargetMetadata,
    isa: &dyn TargetIsa,
) -> Result<PreparedSyntaxModule> {
    with_prepared_module_input(db, front, target, |input, items| lower_registered_module_callables(input, items, isa))
}

/// Lower the reachable union of explicitly selected current-generation source callables.
/// Selection grants no callback or runtime capability; every import retains ordinary admission.
pub fn lower_prepared_syntax_selected_callables(
    db: &mut BeskidDatabase,
    front: &FrontEndTypedResult,
    entries: &[AstNodeKey],
    target: TargetMetadata,
    isa: &dyn TargetIsa,
) -> Result<PreparedSyntaxModule> {
    if entries.is_empty() {
        anyhow::bail!("callable selection is empty");
    }
    let generation = front.syntax_assembly().generation;
    if entries.iter().any(|key| key.generation != generation) {
        anyhow::bail!("callable selection has a stale source generation");
    }
    with_prepared_module_input(db, front, target, |input, items| {
        let mut selected = HashSet::new();
        let mut unique = HashSet::new();
        for key in entries {
            if !unique.insert(*key) || !items.iter().any(|item| item.key == *key) {
                anyhow::bail!("callable selection is duplicate or not a current registered declaration");
            }
            let root = input
                .roots()
                .iter()
                .copied()
                .find(|root| root.unit == key.unit)
                .ok_or_else(|| anyhow::anyhow!("selected callable source root is absent"))?;
            let reachable = beskid_queries::reachable_items(input.database(), root, *key)?
                .ok_or_else(|| anyhow::anyhow!("selected callable reachability is unavailable"))?;
            selected.extend(reachable.iter().copied());
            selected.insert(*key);
        }
        let selected_items = items.iter().filter(|item| selected.contains(&item.key)).cloned().collect::<Vec<_>>();
        lower_registered_module_callables(input, &selected_items, isa)
    })
}

/// Lower the library output of a prepared frontend snapshot.
///
/// The library owns the units named by [`ProgramAssembly::library_unit_indices`]: every own root
/// unit of an entry-less library or aggregate, or the entry plus its own-package units. Every
/// executable item of those units is emitted, and dependency items only when an owned item
/// reaches them, so a dependency shared by several owned units is emitted once. The export set is
/// the `[Export]` declarations of the owned units; a dependency's own `[Export]` items are never
/// re-exported by the consumer. Emission and export order follow assembly order.
pub fn lower_prepared_syntax_library(
    db: &mut BeskidDatabase,
    front: &FrontEndTypedResult,
    target: TargetMetadata,
    isa: &dyn TargetIsa,
) -> Result<CodegenArtifact> {
    with_prepared_module_input(db, front, target, |input, items| {
        let db = input.database();
        let assembly = &input.typed_program().assembly;
        let owned_units = assembly
            .library_unit_indices()
            .ok_or_else(|| anyhow::anyhow!("library root set names a source unit outside the prepared assembly"))?
            .into_iter()
            .map(|index| SourceUnitId::new(db, assembly.units[index].path.clone()))
            .collect::<HashSet<_>>();
        let mut selected = HashSet::new();
        for item in items.iter().filter(|item| owned_units.contains(&item.key.unit)) {
            let root = input
                .roots()
                .iter()
                .copied()
                .find(|root| root.unit == item.key.unit)
                .ok_or_else(|| anyhow::anyhow!("library item `{}` has no source root", item.symbol))?;
            let reachable = reachable_items(db, root, item.key)
                .map_err(|error| anyhow::anyhow!("library reachability query failed for `{}`: {error}", item.symbol))?
                .ok_or_else(|| anyhow::anyhow!("incomplete direct-call facts for library item `{}`", item.symbol))?;
            selected.extend(reachable.iter().copied());
            selected.insert(item.key);
        }
        let mut emitted = items.iter().filter(|item| selected.contains(&item.key)).cloned().collect::<Vec<_>>();
        close_scheduler_entry_reachability(db, input, &mut selected, &mut emitted)?;
        let mut artifact = lower_syntax_program(input, isa, &emitted)
            .map_err(|error| error.into_report(input, "syntax ISLE library lowering failed"))?;
        let owned = emitted.iter().filter(|item| owned_units.contains(&item.key.unit)).cloned().collect::<Vec<_>>();
        artifact.exports = syntax_export_entries(db, &owned)?;
        Ok(artifact)
    })
}

fn lower_registered_module_callables(
    input: &CodegenInput<'_>,
    items: &[SyntaxModuleItem],
    isa: &dyn TargetIsa,
) -> Result<PreparedSyntaxModule> {
    // Type-only libraries have no executable items, but still pass through the
    // validated module boundary and object emission without an application entrypoint.
    let mut artifact = lower_syntax_program(input, isa, items)
        .map_err(|error| error.into_report(&input, "syntax ISLE module lowering failed"))?;
    artifact.exports = syntax_export_entries(input.database(), items)?;
    let mut callables = Vec::with_capacity(items.len());
    let mut link_symbols = HashSet::new();
    let emitted_symbols = artifact.functions.iter().map(|function| function.name.as_str()).collect::<HashSet<_>>();
    for item in items {
        // Only a declaration actually emitted with this symbol can enter the callable table.
        // Generic declarations lacking an instantiated symbol fail closed at this boundary.
        if !emitted_symbols.contains(item.symbol.as_str()) {
            // A generic template may have only call-derived specializations in the artifact.
            // Never issue its uninstantiated declaration as one of those concrete callables.
            continue;
        }
        let signature = item_abi_signature(input.database(), item.key)?
            .ok_or_else(|| anyhow::anyhow!("prepared callable signature unavailable"))?;
        let link_symbol = crate::object_link_symbol(&item.symbol, &artifact.exports);
        if !link_symbols.insert(link_symbol.clone()) {
            anyhow::bail!("prepared callable has a duplicate linker identity: {link_symbol}");
        }
        callables.push(PreparedSyntaxCallable {
            key: item.key,
            internal_symbol: item.symbol.clone(),
            link_symbol,
            signature,
        });
    }
    Ok(PreparedSyntaxModule { artifact, target: input.target().clone(), callables })
}

/// One registered prepared-generation authority shared by module and native adapter lowering.
pub(crate) fn with_prepared_module_input<T>(
    db: &mut BeskidDatabase,
    front: &FrontEndTypedResult,
    target: TargetMetadata,
    execute: impl FnOnce(&CodegenInput<'_>, &[SyntaxModuleItem]) -> Result<T>,
) -> Result<T> {
    let assembly = Arc::new(front.syntax_assembly());
    let generation = assembly.generation;
    let project = project_session_for_syntax_assembly(db, &assembly, "syntax-codegen", "prepared-frontend")
        .map_err(|error| anyhow::anyhow!("syntax program session preparation failed: {error}"))?;
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let capability = canonical_corelib_syscall_service_capability(&manifest)
        .map_err(|error| anyhow::anyhow!("Corelib syscall service capability unavailable: {error:?}"))?;
    let typed = (if assembly.runtime_fixture.is_some() {
        beskid_queries::build_runtime_fixture_typed_program(db, project, generation, Arc::clone(&assembly), &manifest)
    } else {
        build_typed_program_with_corelib_syscall_services(db, project, generation, Arc::clone(&assembly), capability)
    })
    .map_err(|error| anyhow::anyhow!("syntax program preparation failed: {error}"))?;
    let roots = assembly
        .units
        .iter()
        .map(|unit| AstNodeKey {
            unit: SourceUnitId::new(db, unit.path.clone()),
            generation,
            node: beskid_queries::AstNodeId(0),
        })
        .collect::<Vec<_>>();
    let input = CodegenInput::new(db, typed, Arc::from(roots), target.clone(), manifest)
        .map_err(|error| anyhow::anyhow!("invalid syntax codegen input: {error}"))?
        .with_composition_authority(
            generation,
            Arc::new(front.binding_plan.clone()),
            Arc::new(front.composition_snapshot.clone()),
        )
        .map_err(|error| anyhow::anyhow!("invalid composition codegen input: {error}"))?;
    let items = input
        .roots()
        .iter()
        .copied()
        .flat_map(|root| function_definitions(input.database(), root))
        .filter(|key| item_body(input.database(), *key).ok().flatten().is_some())
        .map(|key| syntax_item_symbol(input.database(), &input, key).map(|symbol| SyntaxModuleItem { key, symbol }))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| anyhow::anyhow!("prepared syntax module contains an unnamed item"))?;
    execute(&input, &items)
}

/// Expose the registered prepared-front codegen input to Glue producers.
///
/// This is the only public route from a prepared front end to the registered
/// [`CodegenInput`] and its executable [`SyntaxModuleItem`] closure. It delegates to the same
/// materializer used by module and native adapter lowering, so Glue packets, owner builds and
/// native bodies observe exactly the registered input, its composition authority and its
/// lowering-internal symbols. Every preparation and registration check fails closed; there is
/// no second materializer and callers never reconstruct symbols themselves.
pub fn with_prepared_glue_input<T>(
    db: &mut BeskidDatabase,
    front: &FrontEndTypedResult,
    target: TargetMetadata,
    execute: impl FnOnce(&CodegenInput<'_>, &[SyntaxModuleItem]) -> Result<T>,
) -> Result<T> {
    with_prepared_module_input(db, front, target, execute)
}

/// Preserve `[Export]` facts selected by syntax lowering for AOT/JIT publication.
///
/// The function names in `items` are lowering-internal syntax symbols, while export metadata is
/// keyed by the semantic item itself. Keeping this mapping at the prepared-syntax boundary lets
/// every production syntax module artifact carry the same interop surface as canonical runtime
/// lowering from generation-scoped semantic facts.
fn syntax_export_entries(db: &dyn beskid_queries::Db, items: &[SyntaxModuleItem]) -> Result<Vec<ExportEntry>> {
    syntax_export_entries_matching(db, items, &HashSet::new())
}

fn syntax_export_entries_matching(
    db: &dyn beskid_queries::Db,
    items: &[SyntaxModuleItem],
    allowed_exports: &HashSet<String>,
) -> Result<Vec<ExportEntry>> {
    let mut exports = Vec::new();
    for item in items {
        let export = item_export_symbol(db, item.key)
            .map_err(|error| anyhow::anyhow!("syntax export validation failed: {error}"))?;
        let Some(export) = export else {
            continue;
        };
        if !allowed_exports.is_empty() && !allowed_exports.contains(&*export.0) {
            continue;
        }
        let beskid_name = item_name(db, item.key)
            .map_err(|error| anyhow::anyhow!("syntax export name lookup failed: {error}"))?
            .ok_or_else(|| anyhow::anyhow!("syntax export has no declared function name"))?;
        exports.push(ExportEntry {
            beskid_name: beskid_name.to_string(),
            exported_symbol: export.0.to_string(),
            abi: "C".to_owned(),
        });
    }
    Ok(exports)
}

/// Close the scheduler-entry reachability gap for entrypoint (test/AOT/JIT) lowering.
///
/// [`SCHEDULER_ENTRY_HELPERS`](crate::module_emission::SCHEDULER_ENTRY_HELPERS) are invoked by
/// compiler-generated scheduler entry/return trampolines, not by any source call, so ordinary
/// call-graph reachability from an entrypoint can pull in some of them (via a normal call into
/// the scheduler) while missing others (reached only through indirect context entry). Emission
/// then fails closed with `canonical <name> item unavailable`
/// (`crates/beskid_codegen/src/module_emission/orchestration.rs`). `lower_canonical_runtime_prepared_syntax`
/// avoids this by always adding the helper set as extra reachability roots when building the
/// runtime corpus itself; do the same here whenever this compilation's reachable item set already
/// needs any one of them, so partial scheduler exposure never produces a partial helper set.
/// When a selected item also contains a `spawn`, the generated spawn trampoline calls the
/// [`SCHEDULER_STACK_HELPERS`](crate::module_emission::SCHEDULER_STACK_HELPERS) seams, so they are
/// added as roots too.
fn close_scheduler_entry_reachability(
    db: &dyn beskid_queries::Db,
    input: &CodegenInput<'_>,
    selected: &mut HashSet<AstNodeKey>,
    items: &mut Vec<SyntaxModuleItem>,
) -> Result<()> {
    let already_needed = selected.iter().any(|key| {
        item_name(db, *key)
            .ok()
            .flatten()
            .is_some_and(|name| crate::module_emission::SCHEDULER_ENTRY_HELPERS.contains(&name.as_ref()))
    });
    if !already_needed {
        return Ok(());
    }
    let needs_stack_helpers = selected.iter().any(|key| contains_spawn(db, *key));
    let mut entry_roots = Vec::new();
    for key in input.roots().iter().copied().flat_map(|root| function_definitions(db, root)) {
        if let Some(name) = item_name(db, key)?
            && (crate::module_emission::SCHEDULER_ENTRY_HELPERS.contains(&name.as_ref())
                || (needs_stack_helpers && crate::module_emission::SCHEDULER_STACK_HELPERS.contains(&name.as_ref())))
        {
            entry_roots.push((name.to_string(), key));
        }
    }
    for (export, entry) in &entry_roots {
        let entry = *entry;
        let program = input
            .roots()
            .iter()
            .copied()
            .find(|root| root.unit == entry.unit)
            .ok_or_else(|| anyhow::anyhow!("scheduler entry `{export}` has no source root"))?;
        let reachable = reachable_items(db, program, entry)
            .map_err(|error| anyhow::anyhow!("scheduler entry reachability failed for `{export}`: {error}"))?
            .ok_or_else(|| anyhow::anyhow!("incomplete direct-call facts for scheduler entry `{export}`"))?;
        for key in reachable.iter().copied() {
            if !selected.insert(key) {
                continue;
            }
            let symbol = syntax_item_symbol(db, input, key)
                .ok_or_else(|| anyhow::anyhow!("scheduler entry reachable item is not a syntax function or test"))?;
            items.push(SyntaxModuleItem { key, symbol });
        }
    }
    Ok(())
}

fn contains_spawn(db: &dyn beskid_queries::Db, key: AstNodeKey) -> bool {
    if beskid_queries::node_kind(db, key).ok().flatten() == Some(beskid_queries::IndexedNodeKind::SpawnExpression) {
        return true;
    }
    child_nodes(db, key).ok().flatten().is_some_and(|children| children.iter().any(|child| contains_spawn(db, *child)))
}

fn find_entrypoint(db: &BeskidDatabase, input: &CodegenInput<'_>, entrypoint: &str) -> Option<AstNodeKey> {
    input.roots().iter().copied().find_map(|root| find_item(db, root, entrypoint))
}

fn find_item(db: &BeskidDatabase, key: AstNodeKey, entrypoint: &str) -> Option<AstNodeKey> {
    if item_name(db, key).ok().flatten().as_deref() == Some(entrypoint) {
        return Some(key);
    }
    child_nodes(db, key).ok().flatten()?.iter().copied().find_map(|child| find_item(db, child, entrypoint))
}

fn syntax_item_symbol(db: &dyn beskid_queries::Db, input: &CodegenInput<'_>, key: AstNodeKey) -> Option<String> {
    let name = item_name(db, key).ok().flatten()?;
    let unit = input
        .typed_program()
        .assembly
        .units
        .iter()
        .find(|unit| SourceUnitId::new(db, unit.path.clone()) == key.unit)?;
    let logical = unit
        .logical_name
        .chars()
        .map(|character| if character.is_ascii_alphanumeric() { character } else { '_' })
        .collect::<String>();
    Some(format!("{name}#syntax_{logical}_{}", key.node.0))
}

fn function_definitions(db: &dyn beskid_queries::Db, key: AstNodeKey) -> Vec<AstNodeKey> {
    let mut items = Vec::new();
    if matches!(
        node_kind(db, key).ok().flatten(),
        Some(beskid_queries::IndexedNodeKind::FunctionDefinition | beskid_queries::IndexedNodeKind::MethodDefinition)
    ) {
        items.push(key);
    }
    if let Some(children) = child_nodes(db, key).ok().flatten() {
        for child in children.iter().copied() {
            items.extend(function_definitions(db, child));
        }
    }
    items
}
