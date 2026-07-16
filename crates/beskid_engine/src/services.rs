use std::sync::Arc;

use anyhow::Result;
use beskid_analysis::services::{FrontEndOptions, ResolvedInput};
use beskid_analysis::syntax::{AstNodeId, SyntaxGenerationId};
#[cfg(test)]
use beskid_codegen::module_emission::{SyntaxModuleItem, lower_syntax_program};
use beskid_pipeline::PipelineObserver;
use beskid_queries::{
    AstNodeKey, BeskidDatabase, ProjectSession, SemanticTypeId, build_typed_program, child_nodes,
    item_name, item_signature, test_item, with_db,
};
#[cfg(test)]
use beskid_queries::reachable_items;
use cranelift_codegen::isa::TargetIsa;
use cranelift_codegen::settings;

use crate::Engine;
use crate::jit_callable::{EntryReturnKind, JitCallable};

/// Syntax-backed test metadata consumed by `beskid test`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxTestItem {
    pub name: String,
    pub qualified_name: String,
    pub tags: Vec<String>,
    pub group: Option<String>,
    pub skip_condition: Option<bool>,
    pub skip_reason: Option<String>,
    pub selection_span: beskid_analysis::syntax::SpanInfo,
}

/// Parse, lower, JIT-compile, and run `entrypoint` (no-arg function or test); returns a string summary of the return value.
pub fn run_entrypoint(
    source_path: &std::path::Path,
    source: &str,
    entrypoint: &str,
) -> Result<String> {
    run_entrypoint_with_pipeline(source_path, source, entrypoint, None)
}

/// Same as [`run_entrypoint`] with optional pipeline observation for codegen and JIT phases.
pub fn run_entrypoint_with_pipeline(
    source_path: &std::path::Path,
    source: &str,
    entrypoint: &str,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<String> {
    let source_path = beskid_codegen::materialize_source_path_for_lowering(source_path, source)?;
    let compile_plan = beskid_analysis::services::compile_plan_for_input_path(&source_path)
        .or_else(|| {
            Some(beskid_analysis::services::synthetic_compile_plan_for_source(&source_path))
        });
    let resolved = ResolvedInput {
        source_path,
        source: source.to_string(),
        compile_plan,
        prepared_workspace: None,
        workspace_summary: None,
        assembly: None,
    };
    let front = beskid_queries::compile_front_end_from_resolved_input(
        &resolved,
        FrontEndOptions::default(),
        pipeline,
    )?;
    run_entrypoint_from_front_end_with_pipeline(
        &front,
        &resolved.source_path.display().to_string(),
        &resolved.source,
        entrypoint,
        pipeline,
    )
}

/// JIT-compile and run using a pre-built front-end (avoids re-running semantic analysis).
pub fn run_entrypoint_from_front_end_with_pipeline(
    front: &beskid_analysis::services::FrontEndTypedResult,
    source_name: &str,
    source: &str,
    entrypoint: &str,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<String> {
    let mut engine = Engine::try_new()?;
    run_entrypoint_from_front_end_with_engine(
        &mut engine,
        front,
        source_name,
        source,
        entrypoint,
        pipeline,
    )
}

/// Like [`run_entrypoint_from_front_end_with_pipeline`] but reuses an existing [`Engine`].
pub fn run_entrypoint_from_front_end_with_engine(
    engine: &mut Engine,
    front: &beskid_analysis::services::FrontEndTypedResult,
    source_name: &str,
    source: &str,
    entrypoint: &str,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<String> {
    let syntax_entrypoint = with_db(|db| {
        lower_syntax_entrypoint_from_front_end(
            db,
            front,
            entrypoint,
            engine.target_metadata().clone(),
            pipeline,
        )
    })?;

    // `source_name` and `source` remain part of the public API for compatibility with callers
    // that share this service with diagnostic paths. The production handoff below exclusively
    // consumes the already prepared expanded syntax assembly.
    let _ = (source_name, source);
    run_syntax_jitted_entrypoint(engine, &syntax_entrypoint, entrypoint, pipeline)
}

/// Fully syntax-backed entrypoint authority handed from the prepared frontend to the JIT.
///
/// `symbol` and `return_type` are derived from the same generation-safe item key used to emit
/// CLIF. No HIR node, resolution item id, or legacy codegen entrypoint participates.
struct SyntaxEntrypointArtifact {
    artifact: beskid_codegen::CodegenArtifact,
    symbol: String,
    return_type: SemanticTypeId,
}

fn run_syntax_jitted_entrypoint(
    engine: &mut Engine,
    entrypoint_artifact: &SyntaxEntrypointArtifact,
    entrypoint: &str,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<String> {
    engine
        .compile_artifact_with_pipeline(&entrypoint_artifact.artifact, pipeline)
        .map_err(|err| anyhow::anyhow!("JIT compile failed: {err}"))?;

    let ptr = unsafe { engine.entrypoint_ptr(&entrypoint_artifact.symbol) }
        .map_err(|err| anyhow::anyhow!("Entrypoint lookup failed: {err}"))?;
    if ptr.is_null() {
        return Err(anyhow::anyhow!(
            "Entrypoint `{entrypoint}` returned null pointer"
        ));
    }

    let return_kind = EntryReturnKind::from_semantic_type(entrypoint_artifact.return_type);
    let output = JitCallable::execute_as_i64(ptr, return_kind);
    Ok(JitCallable::format_i64_result(output, return_kind))
}

fn lower_syntax_entrypoint_from_front_end(
    db: &mut BeskidDatabase,
    front: &beskid_analysis::services::FrontEndTypedResult,
    entrypoint: &str,
    target: beskid_abi::abi_v5::TargetMetadata,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<SyntaxEntrypointArtifact> {
    let isa = native_isa()?;
    let lowered = beskid_pipeline::observe_phase_result(
        pipeline,
        beskid_pipeline::phases::CODEGEN_CLIF,
        || beskid_codegen::lower_prepared_syntax_entrypoint(db, front, entrypoint, target, isa.as_ref()),
    )?;
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
    let project = ProjectSession::new(
        db,
        project_root,
        entry_path.clone(),
        "syntax-codegen".into(),
        "prepared-frontend".into(),
    );
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

    let entry_root = AstNodeKey {
        unit: beskid_queries::SourceUnitId::new(db, entry_path),
        generation,
        node: AstNodeId(0),
    };
    let entry = find_syntax_entrypoint(db, &input, entrypoint)
        .ok_or_else(|| anyhow::anyhow!("Missing entrypoint `{entrypoint}`"))?;
    let signature = item_signature(db, entry)
        .map_err(|error| anyhow::anyhow!("entrypoint signature query failed: {error}"))?
        .ok_or_else(|| anyhow::anyhow!("Missing signature for `{entrypoint}`"))?;
    if !signature.parameters.is_empty() {
        return Err(anyhow::anyhow!(
            "Entrypoint `{entrypoint}` must take no parameters"
        ));
    }
    let reachable = reachable_items(db, entry_root, entry)
        .map_err(|error| anyhow::anyhow!("entrypoint reachability query failed: {error}"))?
        .ok_or_else(|| anyhow::anyhow!("incomplete direct-call facts for `{entrypoint}`"))?;
    let items = reachable
        .iter()
        .copied()
        .map(|key| {
            syntax_item_symbol(db, &input, key).map(|symbol| SyntaxModuleItem { key, symbol })
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| anyhow::anyhow!("reachable item is not a syntax function or test"))?;
    let symbol = syntax_item_symbol(db, &input, entry).ok_or_else(|| {
        anyhow::anyhow!("entrypoint `{entrypoint}` is not a syntax function or test")
    })?;
    let isa = native_isa()?;
    let artifact = beskid_pipeline::observe_phase_result(
        pipeline,
        beskid_pipeline::phases::CODEGEN_CLIF,
        || {
            lower_syntax_program(&input, isa.as_ref(), &items)
                .map_err(|error| anyhow::anyhow!("syntax ISLE lowering failed: {error}"))
        },
    )?;
    Ok(SyntaxEntrypointArtifact {
        artifact,
        symbol,
        return_type: signature.result,
    })
}

/// Lower one prepared expanded-syntax entrypoint through the production ISLE boundary.
///
/// Test and tooling callers that only need CLIF can use this without constructing an Engine or
/// a runtime kit. Linked execution must still use [`Engine`]'s exact ABI-v5 kit authority.
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

/// Discover current-generation test items from a prepared frontend snapshot.
///
/// This deliberately registers and queries the post-expansion syntax assembly rather than
/// traversing the legacy HIR-backed `ProgramAssembly` retained for compatibility consumers.
pub fn syntax_test_items_from_front_end(
    front: &beskid_analysis::services::FrontEndTypedResult,
) -> Result<Vec<SyntaxTestItem>> {
    let assembly = Arc::new(front.syntax_assembly());
    with_db(|db| syntax_test_items_from_assembly(db, assembly))
}

/// Return the syntax-derived result type of one prepared no-argument entrypoint.
///
/// REPL type inspection uses this authority directly instead of reading the legacy typed-HIR
/// result retained in the frontend compatibility bundle.
pub fn syntax_entrypoint_return_type_from_front_end(
    front: &beskid_analysis::services::FrontEndTypedResult,
    entrypoint: &str,
) -> Result<SemanticTypeId> {
    let assembly = Arc::new(front.syntax_assembly());
    with_db(|db| {
        let entry_path = assembly.entry_unit().path.clone();
        let project = ProjectSession::new(
            db,
            assembly.roots().host.source_root.clone(),
            entry_path.clone(),
            "syntax-repl".into(),
            "prepared-frontend".into(),
        );
        let generation = SyntaxGenerationId(1);
        build_typed_program(db, project, generation, assembly)
            .map_err(|error| anyhow::anyhow!("syntax REPL preparation failed: {error}"))?;
        let root = AstNodeKey {
            unit: beskid_queries::SourceUnitId::new(db, entry_path),
            generation,
            node: AstNodeId(0),
        };
        let entry = find_syntax_item(db, root, entrypoint)
            .ok_or_else(|| anyhow::anyhow!("Missing entrypoint `{entrypoint}`"))?;
        let signature = item_signature(db, entry)
            .map_err(|error| anyhow::anyhow!("entrypoint signature query failed: {error}"))?
            .ok_or_else(|| anyhow::anyhow!("Missing signature for `{entrypoint}`"))?;
        if !signature.parameters.is_empty() {
            anyhow::bail!("Entrypoint `{entrypoint}` must take no parameters");
        }
        Ok(signature.result)
    })
}

fn syntax_test_items_from_assembly(
    db: &mut BeskidDatabase,
    assembly: Arc<beskid_analysis::projects::SyntaxProgramAssembly>,
) -> Result<Vec<SyntaxTestItem>> {
    let entry_path = assembly.entry_unit().path.clone();
    let project = ProjectSession::new(
        db,
        assembly.roots().host.source_root.clone(),
        entry_path.clone(),
        "syntax-tests".into(),
        "prepared-frontend".into(),
    );
    let generation = SyntaxGenerationId(1);
    build_typed_program(db, project, generation, assembly)
        .map_err(|error| anyhow::anyhow!("syntax test preparation failed: {error}"))?;
    let root = AstNodeKey {
        unit: beskid_queries::SourceUnitId::new(db, entry_path),
        generation,
        node: AstNodeId(0),
    };
    collect_syntax_test_items(db, root)
}

fn collect_syntax_test_items(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
) -> Result<Vec<SyntaxTestItem>> {
    let mut out = Vec::new();
    if let Some(facts) =
        test_item(db, key).map_err(|error| anyhow::anyhow!("syntax test query failed: {error}"))?
    {
        out.push(SyntaxTestItem {
            name: facts.name.to_string(),
            qualified_name: facts.qualified_name.to_string(),
            tags: facts.tags.iter().map(ToString::to_string).collect(),
            group: facts.group.map(|group| group.to_string()),
            skip_condition: facts.skip_condition,
            skip_reason: facts.skip_reason.map(|reason| reason.to_string()),
            selection_span: facts.selection_span,
        });
    }
    for child in child_nodes(db, key)
        .map_err(|error| anyhow::anyhow!("syntax test traversal failed: {error}"))?
        .unwrap_or_default()
        .iter()
        .copied()
    {
        out.extend(collect_syntax_test_items(db, child)?);
    }
    Ok(out)
}

fn native_isa() -> Result<Arc<dyn TargetIsa>> {
    let builder = cranelift_native::builder()
        .map_err(|error| anyhow::anyhow!("native ISA unavailable: {error}"))?;
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
    input
        .roots()
        .iter()
        .copied()
        .find_map(|root| find_syntax_item(db, root, entrypoint))
}

fn find_syntax_item(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
    entrypoint: &str,
) -> Option<AstNodeKey> {
    if syntax_item_name(db, key).as_deref() == Some(entrypoint) {
        return Some(key);
    }
    child_nodes(db, key)
        .ok()
        .flatten()?
        .iter()
        .copied()
        .find_map(|child| find_syntax_item(db, child, entrypoint))
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
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    Some(format!("{name}#syntax_{logical}_{}", key.node.0))
}

fn syntax_item_name(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
) -> Option<String> {
    item_name(db, key)
        .ok()
        .flatten()
        .map(|name| name.as_ref().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use beskid_analysis::projects::{
        AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, RootEntry, SourceUnit,
        SyntaxProgramAssembly,
    };
    use beskid_analysis::services::parse_program_with_source_name;

    #[test]
    fn prepared_syntax_entrypoint_emits_reachable_items_without_hir_lowering() {
        let mut db = BeskidDatabase::default();
        let directory = tempfile::tempdir().expect("project").keep();
        let path = directory.join("Main.bd");
        let source = "i32 Echo(i32 value) { return value; } i32 Main() { return Echo(41); }";
        std::fs::write(&path, source).expect("source");
        let program =
            parse_program_with_source_name(path.to_str().unwrap(), source).expect("parse");
        let assembly = Arc::new(SyntaxProgramAssembly::new(
            EffectiveCompilationRoots {
                host: RootEntry {
                    dependency_name: None,
                    source_root: directory,
                },
                dependencies: Vec::new(),
            },
            Arc::new(vec![SourceUnit {
                logical_name: "Main".into(),
                path,
                source: source.into(),
                program,
            }]),
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

        let lowered = lower_syntax_entrypoint(&mut db, assembly, "Main", target, None)
            .expect("syntax entrypoint lowering");

        assert_eq!(lowered.return_type, SemanticTypeId::I32);
        assert_eq!(lowered.artifact.functions.len(), 2);
        assert!(lowered.symbol.starts_with("Main#syntax_Main_"));
        assert!(
            lowered
                .artifact
                .functions
                .iter()
                .any(|function| function.name.starts_with("Echo#syntax_Main_"))
        );
    }

    #[test]
    fn syntax_test_discovery_preserves_nested_metadata() {
        let mut db = BeskidDatabase::default();
        let directory = tempfile::tempdir().expect("project").keep();
        let path = directory.join("Main.bd");
        let source = r#"mod Checks { test Smoke {
            meta { group = "fast"; tags = "unit, smoke"; }
            skip { condition = true; reason = "not on this host"; }
            return;
        } }"#;
        let program =
            parse_program_with_source_name(path.to_str().unwrap(), source).expect("parse");
        let assembly = Arc::new(SyntaxProgramAssembly::new(
            EffectiveCompilationRoots {
                host: RootEntry {
                    dependency_name: None,
                    source_root: directory,
                },
                dependencies: Vec::new(),
            },
            Arc::new(vec![SourceUnit {
                logical_name: "Main".into(),
                path,
                source: source.into(),
                program,
            }]),
            0,
            AssemblyDiscovery::ImportClosure,
            Arc::new(ModuleIndex::empty()),
            false,
        ));

        let tests = syntax_test_items_from_assembly(&mut db, assembly).expect("syntax tests");
        assert_eq!(tests.len(), 1);
        let test = &tests[0];
        assert_eq!(test.name, "Smoke");
        assert_eq!(test.qualified_name, "Checks::Smoke");
        assert_eq!(test.tags, ["unit", "smoke"]);
        assert_eq!(test.group.as_deref(), Some("fast"));
        assert_eq!(test.skip_condition, Some(true));
        assert_eq!(test.skip_reason.as_deref(), Some("not on this host"));
        assert!(test.selection_span.start < test.selection_span.end);
    }
}
