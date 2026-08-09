use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use beskid_abi::abi_v5::TargetMetadata;
use beskid_analysis::services::{
    FrontEndOptions, FrontEndTypedResult, ResolvedInput, resolved_input_from_plan, synthetic_compile_plan_for_source,
};
#[cfg(test)]
use beskid_analysis::syntax::{AstNodeId, SyntaxGenerationId};
use beskid_codegen::CodegenArtifact;
#[cfg(test)]
use beskid_codegen::module_emission::{SyntaxModuleItem, lower_syntax_program};
use beskid_pipeline::PipelineObserver;
#[cfg(test)]
use beskid_queries::{AstNodeKey, ProjectSession, build_typed_program, item_signature, reachable_items};
use beskid_queries::{BeskidDatabase, SemanticTypeId, with_db};
use cranelift_codegen::isa::TargetIsa;
use cranelift_codegen::settings;

use super::SyntaxEntrypointArtifact;
#[cfg(test)]
use super::syntax_queries::{find_syntax_item, syntax_item_name};

/// CodegenInput → ISLE artifact prepared for exact-kit JIT compilation.
#[derive(Debug)]
pub struct PreparedJitEntrypoint {
    pub artifact: CodegenArtifact,
    pub symbol: String,
    pub return_type: SemanticTypeId,
    pub target: TargetMetadata,
}

/// Prepare a no-arg entrypoint through the sole CodegenInput → ISLE route (no HIR/`Lowerable`).
pub fn prepare_jit_entrypoint(source_path: &Path, source: &str, entrypoint: &str) -> Result<PreparedJitEntrypoint> {
    let front = prepare_syntax_front_end(source_path, source)?;
    let target =
        crate::host_runtime_target().map_err(|error| anyhow::anyhow!("host ABI-v5 target unavailable: {error}"))?;
    let lowered = with_db(|db| lower_syntax_entrypoint_from_front_end(db, &front, entrypoint, target.clone(), None))?;
    Ok(PreparedJitEntrypoint {
        artifact: lowered.artifact,
        symbol: lowered.symbol,
        return_type: lowered.return_type,
        target,
    })
}

/// Prepare every executable item in a snippet through CodegenInput → ISLE module emission.
pub fn prepare_jit_module(source_path: &Path, source: &str) -> Result<CodegenArtifact> {
    let front = prepare_syntax_front_end(source_path, source)?;
    let target =
        crate::host_runtime_target().map_err(|error| anyhow::anyhow!("host ABI-v5 target unavailable: {error}"))?;
    let isa = native_isa()?;
    with_db(|db| beskid_codegen::lower_prepared_syntax_module(db, &front, target, isa.as_ref()))
}

/// Front-end prepare for JIT/REPL snippets; semantic diagnostics stay enabled so invalid
/// extern contracts fail closed before CodegenInput construction.
pub fn prepare_syntax_front_end(source_path: &Path, source: &str) -> Result<FrontEndTypedResult> {
    let source_path = beskid_codegen::materialize_source_path_for_lowering(source_path, source)?;
    let plan = synthetic_compile_plan_for_source(&source_path);
    let resolved: ResolvedInput = resolved_input_from_plan(source_path, source.to_owned(), plan, None, None);
    beskid_queries::compile_front_end_from_resolved_input(
        &resolved,
        FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
        None,
    )
}

pub(super) fn lower_syntax_entrypoint_from_front_end(
    db: &mut BeskidDatabase,
    front: &beskid_analysis::services::FrontEndTypedResult,
    entrypoint: &str,
    target: beskid_abi::abi_v5::TargetMetadata,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<SyntaxEntrypointArtifact> {
    let isa = native_isa()?;
    let lowered = beskid_pipeline::observe_phase_result(pipeline, beskid_pipeline::phases::CODEGEN_CLIF, || {
        beskid_codegen::lower_prepared_syntax_entrypoint(db, front, entrypoint, target, isa.as_ref())
    })?;
    Ok(SyntaxEntrypointArtifact {
        artifact: lowered.artifact,
        symbol: lowered.symbol,
        return_type: lowered.return_type,
    })
}

#[cfg(test)]
fn lower_syntax_entrypoint(
    db: &mut BeskidDatabase,
    syntax_assembly: Arc<beskid_analysis::projects::SyntaxProgramAssembly>,
    entrypoint: &str,
    target: beskid_abi::abi_v5::TargetMetadata,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<SyntaxEntrypointArtifact> {
    let entry_path = syntax_assembly.entry_unit().path.clone();
    let project_root = syntax_assembly.roots().host.source_root.clone();
    let project =
        ProjectSession::new(db, project_root, entry_path.clone(), "syntax-codegen".into(), "prepared-frontend".into());
    // The prepared frontend assembly is immutable for this JIT request. A fresh local
    // compilation database is used by the caller, so this generation cannot alias a prior
    // revision; all item identities below still carry it explicitly.
    let generation = SyntaxGenerationId(1);
    let typed = build_typed_program(db, project, generation, Arc::clone(&syntax_assembly))
        .map_err(|error| anyhow::anyhow!("syntax program preparation failed: {error}"))?;
    let roots = syntax_assembly
        .units()
        .iter()
        .map(|unit| AstNodeKey {
            unit: beskid_queries::SourceUnitId::new(db, unit.path.clone()),
            generation,
            node: AstNodeId(0),
        })
        .collect::<Vec<_>>();
    let input = beskid_codegen::CodegenInput::new(
        db,
        typed,
        Arc::from(roots),
        target.clone(),
        beskid_abi::abi_v5::AbiManifestV5::canonical_runtime(target),
    )
    .map_err(|error| anyhow::anyhow!("invalid syntax codegen input: {error}"))?;

    let entry_root =
        AstNodeKey { unit: beskid_queries::SourceUnitId::new(db, entry_path), generation, node: AstNodeId(0) };
    let entry = find_syntax_entrypoint(db, &input, entrypoint)
        .ok_or_else(|| anyhow::anyhow!("Missing entrypoint `{entrypoint}`"))?;
    let signature = item_signature(db, entry)
        .map_err(|error| anyhow::anyhow!("entrypoint signature query failed: {error}"))?
        .ok_or_else(|| anyhow::anyhow!("Missing signature for `{entrypoint}`"))?;
    if !signature.parameters.is_empty() {
        return Err(anyhow::anyhow!("Entrypoint `{entrypoint}` must take no parameters"));
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
    let isa = native_isa()?;
    let artifact = beskid_pipeline::observe_phase_result(pipeline, beskid_pipeline::phases::CODEGEN_CLIF, || {
        lower_syntax_program(&input, isa.as_ref(), &items)
            .map_err(|error| anyhow::anyhow!("syntax ISLE lowering failed: {error}"))
    })?;
    Ok(SyntaxEntrypointArtifact { artifact, symbol, return_type: signature.result })
}

/// Lower one prepared expanded-syntax entrypoint through the production ISLE boundary.
///
/// Test and tooling callers that only need CLIF can use this without constructing an Engine or
/// a runtime kit. Linked execution must still use [`crate::Engine`]'s exact ABI-v5 kit authority.
pub fn lower_prepared_syntax_entrypoint(
    front: &beskid_analysis::services::FrontEndTypedResult,
    entrypoint: &str,
    target: beskid_abi::abi_v5::TargetMetadata,
) -> Result<beskid_codegen::CodegenArtifact> {
    with_db(|db| {
        lower_syntax_entrypoint_from_front_end(db, front, entrypoint, target, None)
            .map(|entrypoint| entrypoint.artifact)
    })
}

/// Lower a preassembled syntax entrypoint through the host ISLE boundary.
///
/// Corelib migration gates use this when the generation-safe syntax registry has authority for
/// a dependency graph that the retired HIR resolver cannot represent. No HIR frontend is
/// prepared as a fallback.
pub fn lower_syntax_assembly_entrypoint(
    assembly: Arc<beskid_analysis::projects::SyntaxProgramAssembly>,
    entrypoint: &str,
    target: beskid_abi::abi_v5::TargetMetadata,
) -> Result<beskid_codegen::CodegenArtifact> {
    with_db(|db| {
        let isa = native_isa()?;
        beskid_codegen::lower_syntax_assembly_entrypoint(db, assembly, entrypoint, target, isa.as_ref())
            .map(|entrypoint| entrypoint.artifact)
    })
}

fn native_isa() -> Result<Arc<dyn TargetIsa>> {
    let builder = cranelift_native::builder().map_err(|error| anyhow::anyhow!("native ISA unavailable: {error}"))?;
    builder
        .finish(settings::Flags::new(settings::builder()))
        .map_err(|error| anyhow::anyhow!("native ISA construction failed: {error}"))
}

#[cfg(test)]
fn find_syntax_entrypoint(
    db: &dyn beskid_queries::Db,
    input: &beskid_codegen::CodegenInput<'_>,
    entrypoint: &str,
) -> Option<AstNodeKey> {
    input.roots().iter().copied().find_map(|root| find_syntax_item(db, root, entrypoint))
}

#[cfg(test)]
fn syntax_item_symbol(
    db: &dyn beskid_queries::Db,
    input: &beskid_codegen::CodegenInput<'_>,
    key: AstNodeKey,
) -> Option<String> {
    let name = syntax_item_name(db, key)?;
    let unit = input
        .typed_program()
        .assembly
        .units()
        .iter()
        .find(|unit| beskid_queries::SourceUnitId::new(db, unit.path.clone()) == key.unit)?;
    let logical = unit
        .logical_name
        .chars()
        .map(|character| if character.is_ascii_alphanumeric() { character } else { '_' })
        .collect::<String>();
    Some(format!("{name}#syntax_{logical}_{}", key.node.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use beskid_analysis::projects::{
        AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, RootEntry, SourceUnit, SyntaxProgramAssembly,
    };
    use beskid_analysis::services::parse_program_with_source_name;

    #[test]
    fn prepared_syntax_entrypoint_emits_reachable_items_without_hir_lowering() {
        let mut db = BeskidDatabase::default();
        let directory = tempfile::tempdir().expect("project").keep();
        let path = directory.join("Main.bd");
        let source = "i32 Echo(i32 value) { return value; } i32 Main() { return Echo(41); }";
        std::fs::write(&path, source).expect("source");
        let program = parse_program_with_source_name(path.to_str().unwrap(), source).expect("parse");
        let assembly = Arc::new(SyntaxProgramAssembly::new(
            EffectiveCompilationRoots {
                host: RootEntry { dependency_name: None, source_root: directory },
                dependencies: Vec::new(),
            },
            Arc::new(vec![SourceUnit { logical_name: "Main".into(), path, source: source.into(), program }]),
            0,
            AssemblyDiscovery::ImportClosure,
            Arc::new(ModuleIndex::empty()),
            false,
        ));
        let host_triple = match (std::env::consts::ARCH, std::env::consts::OS) {
            ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
            ("aarch64", "macos") => "aarch64-apple-darwin",
            ("x86_64", "windows") => "x86_64-pc-windows-msvc",
            _ => panic!("unsupported ABI-v5 test host"),
        };
        let target = beskid_abi::abi_v5::TargetMetadata::supported()
            .into_iter()
            .find(|candidate| candidate.triple.as_str() == host_triple)
            .expect("host ABI-v5 target");

        let lowered =
            lower_syntax_entrypoint(&mut db, assembly, "Main", target, None).expect("syntax entrypoint lowering");

        assert_eq!(lowered.return_type, SemanticTypeId::I32);
        assert_eq!(lowered.artifact.functions.len(), 2);
        assert!(lowered.symbol.starts_with("Main#syntax_Main_"));
        assert!(lowered.artifact.functions.iter().any(|function| function.name.starts_with("Echo#syntax_Main_")));
    }
}
