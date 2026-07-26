//! Host-neutral prepared-syntax entrypoint lowering.

use std::sync::Arc;

use anyhow::Result;
use beskid_abi::{
    abi_v5::{AbiManifestV5, TargetMetadata},
    runtime_source::{
        CANONICAL_BOOTSTRAP_SOURCE_PATH, canonical_corelib_syscall_service_capability,
        canonical_runtime_intrinsic_capability, canonical_runtime_sources,
    },
};
use beskid_analysis::{
    projects::{
        AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, RootEntry, SourceUnit, SyntaxProgramAssembly,
    },
    services::{FrontEndTypedResult, parse_program_with_source_name},
};
use beskid_isle::AstNodeKey;
use beskid_queries::{
    BeskidDatabase, SemanticTypeId, SourceUnitId, SyntaxGenerationId, build_canonical_runtime_typed_program,
    build_typed_program_with_corelib_syscall_services, child_nodes, item_body, item_export_symbol, item_name,
    item_signature, node_kind, project_session_for_syntax_assembly, reachable_items,
};
use cranelift_codegen::isa::TargetIsa;

use crate::{CodegenArtifact, CodegenInput, ExportEntry, SyntaxModuleItem, lower_syntax_program};

/// Result of lowering one prepared syntax entrypoint through the HIR-free boundary.
pub struct PreparedSyntaxEntrypoint {
    pub artifact: CodegenArtifact,
    pub symbol: String,
    pub return_type: SemanticTypeId,
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
            let program = parse_program_with_source_name(path.to_str().unwrap_or_default(), &source.source)
                .map_err(|error| anyhow::anyhow!("canonical runtime parse failed for {}: {error}", source.logical_path))?;
            Ok(SourceUnit { logical_name: source.logical_path, path, source: source.source, program })
        })
        .collect::<Result<Vec<_>>>()?;
    let bootstrap_index = units
        .iter()
        .position(|unit| unit.path == bootstrap_path)
        .ok_or_else(|| anyhow::anyhow!("parsed canonical Bootstrap source is missing"))?;
    let generation = SyntaxGenerationId(1);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root_dir },
            dependencies: Vec::new(),
        },
        Arc::new(units),
        bootstrap_index,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let project = project_session_for_syntax_assembly(db, &assembly, "beskid-runtime-native", "canonical-runtime")
        .map_err(|error| anyhow::anyhow!("canonical runtime session preparation failed: {error}"))?;
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let capability = canonical_runtime_intrinsic_capability(&manifest)
        .map_err(|error| anyhow::anyhow!("canonical runtime intrinsic capability unavailable: {error:?}"))?;
    let typed = build_canonical_runtime_typed_program(db, project, generation, assembly.clone(), capability)
        .map_err(|error| anyhow::anyhow!("canonical runtime syntax preparation failed: {error}"))?;
    // Runtime ABI exports may be implemented by their owning canonical module rather than
    // Bootstrap. Lower every embedded runtime unit so direct calls remain unit-local and the
    // resulting artifact retains the complete manifest-facing export surface.
    let roots = assembly
        .units()
        .iter()
        .map(|unit| AstNodeKey {
            unit: SourceUnitId::new(db, unit.path.clone()),
            generation,
            node: beskid_queries::AstNodeId(0),
        })
        .collect::<Vec<_>>();
    let input = CodegenInput::new(db, typed, Arc::from(roots), target, manifest)
        .map_err(|error| anyhow::anyhow!("canonical runtime CodegenInput failed: {error}"))?;
    let mut items = Vec::new();
    for key in input
        .roots()
        .iter()
        .copied()
        .flat_map(|root| function_definitions(input.database(), root))
    {
        let export = item_export_symbol(input.database(), key)
            .map_err(|error| anyhow::anyhow!("canonical runtime export validation failed: {error}"))?;
        let symbol =
            export.map(|symbol| symbol.0.to_string()).or_else(|| syntax_item_symbol(input.database(), &input, key));
        if let Some(symbol) = symbol {
            items.push(SyntaxModuleItem { key, symbol });
        }
    }
    if items.is_empty() {
        anyhow::bail!("canonical runtime source corpus has no declared exports");
    }
    let mut artifact = lower_syntax_program(&input, isa, &items)
        .map_err(|error| anyhow::anyhow!("canonical runtime ISLE lowering failed: {error}"))?;
    artifact.exports = syntax_export_entries(input.database(), &items)?;
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
    lower_syntax_assembly_entrypoint(db, assembly, entrypoint, target, isa)
}

/// Lower one entrypoint from an assembled syntax program through ISLE.
///
/// This is the executable boundary for callers which already own a
/// [`SyntaxProgramAssembly`]. It intentionally does not accept a typed-HIR frontend or invoke
/// the legacy prepare spine: each semantic and reachability fact is derived from the supplied
/// generation-scoped syntax assembly.
pub fn lower_syntax_assembly_entrypoint(
    db: &mut BeskidDatabase,
    assembly: Arc<SyntaxProgramAssembly>,
    entrypoint: &str,
    target: TargetMetadata,
    isa: &dyn TargetIsa,
) -> Result<PreparedSyntaxEntrypoint> {
    let entry_path = assembly.entry_unit().path.clone();
    let generation = SyntaxGenerationId(1);
    let project = project_session_for_syntax_assembly(db, &assembly, "syntax-codegen", "prepared-frontend")
        .map_err(|error| anyhow::anyhow!("syntax program session preparation failed: {error}"))?;
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let capability = canonical_corelib_syscall_service_capability(&manifest)
        .map_err(|error| anyhow::anyhow!("Corelib syscall service capability unavailable: {error:?}"))?;
    let typed =
        build_typed_program_with_corelib_syscall_services(db, project, generation, Arc::clone(&assembly), capability)
            .map_err(|error| anyhow::anyhow!("syntax program preparation failed: {error}"))?;
    let roots = assembly
        .units()
        .iter()
        .map(|unit| AstNodeKey {
            unit: SourceUnitId::new(db, unit.path.clone()),
            generation,
            node: beskid_queries::AstNodeId(0),
        })
        .collect::<Vec<_>>();
    let input = CodegenInput::new(db, typed, Arc::from(roots), target.clone(), manifest)
        .map_err(|error| anyhow::anyhow!("invalid syntax codegen input: {error}"))?;
    let entry_root =
        AstNodeKey { unit: SourceUnitId::new(db, entry_path), generation, node: beskid_queries::AstNodeId(0) };
    let entry =
        find_entrypoint(db, &input, entrypoint).ok_or_else(|| anyhow::anyhow!("Missing entrypoint `{entrypoint}`"))?;
    crate::isle_trace::event(|| {
        format!(
            "event=entry.selected entrypoint={entrypoint} key={}#g{}:n{}",
            entry.unit.path(db).display(),
            entry.generation.0,
            entry.node.0,
        )
    });
    let signature = item_signature(db, entry)
        .map_err(|error| anyhow::anyhow!("entrypoint signature query failed: {error}"))?
        .ok_or_else(|| anyhow::anyhow!("Missing signature for `{entrypoint}`"))?;
    if !signature.parameters.is_empty() {
        anyhow::bail!("Entrypoint `{entrypoint}` must take no parameters");
    }
    let reachable = reachable_items(db, entry_root, entry)
        .map_err(|error| anyhow::anyhow!("entrypoint reachability query failed: {error}"))?
        .ok_or_else(|| anyhow::anyhow!("incomplete direct-call facts for `{entrypoint}`"))?;
    let items = reachable
        .iter()
        .copied()
        .map(|key| syntax_item_symbol(db, &input, key).map(|symbol| SyntaxModuleItem { key, symbol }))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| anyhow::anyhow!("reachable item is not a syntax function or test"))?;
    let symbol = syntax_item_symbol(db, &input, entry)
        .ok_or_else(|| anyhow::anyhow!("entrypoint `{entrypoint}` is not a syntax function or test"))?;
    let artifact = lower_syntax_program(&input, isa, &items)
        .map_err(|error| anyhow::anyhow!("syntax ISLE lowering failed: {error}"))?;
    Ok(PreparedSyntaxEntrypoint { artifact, symbol, return_type: signature.result })
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
    let assembly = Arc::new(front.syntax_assembly());
    let generation = SyntaxGenerationId(1);
    let project = project_session_for_syntax_assembly(db, &assembly, "syntax-codegen", "prepared-frontend")
        .map_err(|error| anyhow::anyhow!("syntax program session preparation failed: {error}"))?;
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let capability = canonical_corelib_syscall_service_capability(&manifest)
        .map_err(|error| anyhow::anyhow!("Corelib syscall service capability unavailable: {error:?}"))?;
    let typed =
        build_typed_program_with_corelib_syscall_services(db, project, generation, Arc::clone(&assembly), capability)
            .map_err(|error| anyhow::anyhow!("syntax program preparation failed: {error}"))?;
    let roots = assembly
        .units()
        .iter()
        .map(|unit| AstNodeKey {
            unit: SourceUnitId::new(db, unit.path.clone()),
            generation,
            node: beskid_queries::AstNodeId(0),
        })
        .collect::<Vec<_>>();
    let input = CodegenInput::new(db, typed, Arc::from(roots), target.clone(), manifest)
        .map_err(|error| anyhow::anyhow!("invalid syntax codegen input: {error}"))?;
    let items = input
        .roots()
        .iter()
        .copied()
        .flat_map(|root| function_definitions(input.database(), root))
        .filter(|key| item_body(input.database(), *key).ok().flatten().is_some())
        .map(|key| syntax_item_symbol(input.database(), &input, key).map(|symbol| SyntaxModuleItem { key, symbol }))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| anyhow::anyhow!("prepared syntax module contains an unnamed item"))?;
    if items.is_empty() {
        anyhow::bail!("prepared syntax module contains no executable functions or methods");
    }
    let mut artifact = lower_syntax_program(&input, isa, &items)
        .map_err(|error| anyhow::anyhow!("syntax ISLE module lowering failed: {error}"))?;
    artifact.exports = syntax_export_entries(input.database(), &items)?;
    Ok(artifact)
}

/// Preserve `[Export]` facts selected by syntax lowering for AOT/JIT publication.
///
/// The function names in `items` are lowering-internal syntax symbols, while export metadata is
/// keyed by the semantic item itself. Keeping this mapping at the prepared-syntax boundary lets
/// every production syntax module artifact carry the same interop surface as canonical runtime
/// lowering without reconstructing retired HIR state.
fn syntax_export_entries(db: &dyn beskid_queries::Db, items: &[SyntaxModuleItem]) -> Result<Vec<ExportEntry>> {
    let mut exports = Vec::new();
    for item in items {
        let export = item_export_symbol(db, item.key)
            .map_err(|error| anyhow::anyhow!("syntax export validation failed: {error}"))?;
        let Some(export) = export else {
            continue;
        };
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
        .units()
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
