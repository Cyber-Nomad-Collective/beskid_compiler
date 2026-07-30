use std::{collections::BTreeSet, sync::Arc};

use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};
use beskid_analysis::{
    projects::{
        AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, RootEntry, SourceUnit, SyntaxProgramAssembly,
    },
    services::parse_program_with_source_name,
};
use beskid_codegen::lower_canonical_runtime_prepared_syntax;
use beskid_codegen::lower_syntax_assembly_entrypoint;
use beskid_queries::{
    AstNodeId, AstNodeKey, SourceUnitId, SyntaxGenerationId, child_nodes, closure_environment, node_kind, with_db,
};
use cranelift_codegen::{isa, settings, verify_function};

#[test]
fn retired_public_codegen_facade_is_absent() {
    let public_services = include_str!("../src/services.rs");
    for retired_api in [
        "pub struct LoweredProgram",
        "pub fn lower_source",
        "pub fn lower_source_for_entrypoint",
        "pub fn lower_source_with_pipeline",
        "pub fn lower_resolved_input_with_pipeline",
        "pub fn lower_from_prepared_or_cache",
        "pub fn lower_resolved_entrypoint_with_pipeline",
        "pub fn lower_from_front_end",
    ] {
        assert!(
            !public_services.contains(retired_api),
            "retired public codegen facade must not expose `{retired_api}`"
        );
    }

    let public_exports = include_str!("../src/lib.rs");
    assert!(
        !public_exports.contains("    Lowerable,"),
        "the internal Lowerable trait must not be re-exported publicly"
    );
}

fn parse_production_units(root: &std::path::Path, units: &[(&str, &str, &str)]) -> Arc<SyntaxProgramAssembly> {
    let mut source_units = Vec::with_capacity(units.len());
    for (relative_path, logical_name, source) in units {
        let path = root.join(relative_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("unit parent directory");
        }
        std::fs::write(&path, source).expect("write project source");
        let program = parse_program_with_source_name(path.to_str().expect("UTF-8 source path"), source)
            .expect("production source parse");
        source_units.push(SourceUnit { logical_name: (*logical_name).into(), path, source: (*source).into(), program });
    }
    Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.to_path_buf() },
            dependencies: Vec::new(),
        },
        Arc::new(source_units),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ))
}

fn x86_64_target_and_isa() -> (TargetMetadata, std::sync::Arc<dyn cranelift_codegen::isa::TargetIsa>) {
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("Linux x86_64 ABI target");
    let isa = isa::lookup_by_name("x86_64")
        .expect("x86 ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("finish ISA");
    (target, isa)
}

fn lower_verified_entrypoint(
    assembly: Arc<SyntaxProgramAssembly>,
    target: TargetMetadata,
    isa: &dyn cranelift_codegen::isa::TargetIsa,
) -> beskid_codegen::PreparedSyntaxEntrypoint {
    let lowered = with_db(|db| lower_syntax_assembly_entrypoint(db, assembly, "Main", target, isa))
        .expect("parsed project lowers through CodegenInput and ISLE");
    assert!(
        lowered.symbol.starts_with("Main#syntax_"),
        "production path must mint a syntax-mangled entry symbol, got {}",
        lowered.symbol
    );
    for function in &lowered.artifact.functions {
        verify_function(&function.function, isa.flags())
            .unwrap_or_else(|error| panic!("stock CLIF verifier rejected {}: {error}", function.name));
    }
    lowered
}

fn assert_unsupported_closed_failure(
    assembly: Arc<SyntaxProgramAssembly>,
    target: TargetMetadata,
    isa: &dyn cranelift_codegen::isa::TargetIsa,
    expected_site_fragments: &[&str],
) {
    let result = with_db(|db| lower_syntax_assembly_entrypoint(db, assembly, "Main", target, isa));
    let error = match result {
        Ok(_) => panic!("unsupported typed operation must not fall back to legacy codegen"),
        Err(error) => error,
    };
    let rendered = error.to_string();
    assert!(rendered.contains("MissingRuleOrFact"), "{rendered}");
    for fragment in expected_site_fragments {
        assert!(rendered.contains(fragment), "expected {fragment:?} in {rendered}");
    }
}

#[test]
fn parsed_project_reaches_verified_isle_without_a_legacy_codegen_entrypoint() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        type Pair { i32 left, i32 right }
        i32 Add(i32 left, i32 right) { return left + right; }
        i32 Main() {
            Pair pair = Pair { left: 19, right: 23 };
            if pair.left < pair.right { return Add(pair.left, pair.right); }
            return 0;
        }
    ";
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    // The production syntax-only entrypoint accepts only parsed SyntaxProgramAssembly data: no
    // HIR or Lowerable value is constructed or supplied to the code-generation route.
    let lowered = lower_verified_entrypoint(assembly, target.clone(), isa.as_ref());
    assert_eq!(lowered.artifact.functions.len(), 2, "reachable direct-call closure");

    let unsupported_source = "
        i32 Main() {
            i32 outer = 1;
            let task = spawn ((i32 inner) => outer + inner);
            return outer;
        }
    ";
    let unsupported = parse_production_units(project.path(), &[("Unsupported.bd", "Main", unsupported_source)]);
    // `lower_nested_statement` retains the leaf statement key, so the closed failure must name the
    // `let` that spawns the parameterized lambda rather than the enclosing function body block.
    assert_unsupported_closed_failure(unsupported, target, isa.as_ref(), &["Unsupported.bd", "LetStatement@"]);
}

#[test]
fn parsed_direct_pointer_guard_with_unit_early_return_emits_verified_clif() {
    // Keep this shape aligned with the canonical scheduler's initialization guard:
    // `mut pointer table = SchedTable(); if table != NativePointer(0) { return; }`.
    // The recursive conversion helper is never executed; it supplies the exact direct-call
    // ABI shape so this test exercises statement lowering and imports rather than a host shim.
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        pointer NativePointer(word value) { return NativePointer(value); }
        pointer SchedTable() { return NativePointer(0); }
        unit Main() {
            mut pointer table = SchedTable();
            if table != NativePointer(0) { return; }
            return;
        }
    ";
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    let main = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Main#syntax_"))
        .expect("Main artifact");
    let clif = main.function.display().to_string();
    assert!(clif.contains("call"), "direct SchedTable/NativePointer calls must lower: {clif}");
    assert!(clif.contains("brif"), "the no-else pointer guard must branch in CLIF: {clif}");
}

#[test]
fn parsed_direct_zero_argument_spawn_emits_syntax_owned_trampoline_and_fiber_dispatch() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        i64 Entry() { return 7_i64; }
        i64 Main() { return spawn Entry; }
    ";
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    assert_eq!(lowered.artifact.functions.len(), 3, "Entry, Main, and the syntax-owned spawn trampoline");
    let main = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Main#syntax_"))
        .expect("Main artifact");
    let trampoline = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("__beskid_spawn_entry_syntax_"))
        .expect("syntax-owned spawn trampoline");
    let main_clif = main.function.display().to_string();
    let trampoline_clif = trampoline.function.display().to_string();
    assert!(main_clif.contains("beskid_rt_v5_fiber_spawn_with_cancel_slot"), "{main_clif}");
    assert!(!main_clif.contains("interop_dispatch_"), "{main_clif}");
    assert!(main_clif.contains("func_addr"), "{main_clif}");
    assert!(trampoline_clif.contains("Entry#syntax_"), "{trampoline_clif}");
    assert!(trampoline_clif.contains("return"), "{trampoline_clif}");
}

#[test]
fn parsed_zero_capture_lambda_spawn_emits_syntax_owned_entry_and_fiber_dispatch() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        i64 Main() { return spawn (() => 7_i64); }
    ";
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    assert_eq!(lowered.artifact.functions.len(), 3, "Main, the syntax-owned lambda entry, and its spawn trampoline");
    let main = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Main#syntax_"))
        .expect("Main artifact");
    let lambda = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("__beskid_spawn_lambda_syntax_"))
        .expect("syntax-owned lambda entry");
    let trampoline = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("__beskid_spawn_entry_syntax_"))
        .expect("syntax-owned spawn trampoline");
    let main_clif = main.function.display().to_string();
    let lambda_clif = lambda.function.display().to_string();
    let trampoline_clif = trampoline.function.display().to_string();
    assert!(main_clif.contains("beskid_rt_v5_fiber_spawn_with_cancel_slot"), "{main_clif}");
    assert!(!main_clif.contains("interop_dispatch_"), "{main_clif}");
    assert!(main_clif.contains("func_addr"), "{main_clif}");
    assert!(lambda_clif.contains("iconst.i64 7"), "{lambda_clif}");
    assert!(trampoline_clif.contains("__beskid_spawn_lambda_syntax_"), "{trampoline_clif}");
}

#[test]
fn parsed_capturing_lambda_spawn_allocates_roots_and_dispatches_fiber_entry() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        i64 Main() { i64 outer = 41_i64; return spawn (() => outer); }
    ";
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    assert!(
        !lowered.artifact.closure_static_plans.is_empty(),
        "capturing spawn must materialize a generation-safe closure static plan"
    );
    assert_eq!(
        lowered.artifact.closure_static_plans[0].captures.len(),
        1,
        "outer capture must appear in the static plan"
    );
    let main = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Main#syntax_"))
        .expect("Main artifact");
    let lambda = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("__beskid_spawn_lambda_syntax_"))
        .expect("syntax-owned capturing lambda entry");
    let trampoline = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("__beskid_spawn_entry_syntax_"))
        .expect("syntax-owned spawn trampoline");
    let main_clif = main.function.display().to_string();
    let lambda_clif = lambda.function.display().to_string();
    let trampoline_clif = trampoline.function.display().to_string();
    assert!(main_clif.contains("beskid_rt_v5_closure_environment_allocate"), "{main_clif}");
    assert!(main_clif.contains("beskid_rt_v5_closure_environment_root_current"), "{main_clif}");
    assert!(main_clif.contains("beskid_rt_v5_fiber_spawn_with_cancel_slot"), "{main_clif}");
    assert!(!main_clif.contains("interop_dispatch_"), "{main_clif}");
    assert!(
        lambda_clif.contains("load") || lambda_clif.contains("ireduce") || lambda_clif.contains("iadd"),
        "lambda entry must read the rooted capture environment: {lambda_clif}"
    );
    assert!(trampoline_clif.contains("__beskid_spawn_lambda_syntax_"), "{trampoline_clif}");
}

#[test]
fn multi_unit_parsed_project_lowers_through_codegen_input_isle_only() {
    let project = tempfile::tempdir().expect("project directory");
    let util_source = "pub i32 Double(i32 value) { return value + value; }";
    let main_source = "
        use Util;
        i32 Main() {
            return Util.Double(21);
        }
    ";
    let assembly =
        parse_production_units(project.path(), &[("Main.bd", "Main", main_source), ("Util.bd", "Util", util_source)]);
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    assert!(lowered.artifact.functions.len() >= 2, "reachable closure must include Main and imported Util.Double");
}

#[test]
fn parsed_project_control_flow_while_break_continue_reaches_verified_clif() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        i32 Main() {
            mut i32 i = 0;
            mut i32 sum = 0;
            while i < 5 {
                i = i + 1;
                if i == 2 { continue; }
                if i == 4 { break; }
                sum = sum + i;
            }
            return sum;
        }
    ";
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    let main = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Main#syntax_"))
        .expect("Main artifact function");
    let clif = main.function.display().to_string();
    assert!(clif.contains("brif"), "expected while/if branching: {clif}");
    assert!(clif.matches("jump").count() >= 2, "expected loop transfer jumps: {clif}");
}

#[test]
fn parsed_project_if_else_reaches_verified_clif() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        i32 Main() {
            i32 value = 3;
            if value < 2 {
                return 1;
            } else {
                return 7;
            }
        }
    ";
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    let main = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Main#syntax_"))
        .expect("Main artifact function");
    let clif = main.function.display().to_string();
    assert!(clif.contains("brif"), "expected if/else branching: {clif}");
}

#[test]
fn parsed_project_range_for_accumulator_reaches_verified_clif_without_hir_fallback() {
    // Mutable assignment inside a parsed range body is governed solely by generation-bound
    // syntax facts. The production path must not use HIR/Lowerable as a fallback.
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        i32 Main() {
            mut i32 sum = 0;
            for i in range(0, 4) {
                sum = sum + i;
            }
            return sum;
        }
    ";
    let assembly = parse_production_units(project.path(), &[("RangeFor.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();
    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    let main = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Main#syntax_"))
        .expect("Main artifact function");
    let clif = main.function.display().to_string();
    assert!(clif.contains("brif"), "expected range-for branch: {clif}");
    assert!(clif.contains("iadd"), "expected accumulator addition: {clif}");
}

#[test]
fn parsed_project_nested_direct_calls_reach_verified_clif() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        i32 Inner(i32 value) { return value + 1; }
        i32 Mid(i32 value) { return Inner(value) + Inner(value); }
        i32 Main() { return Mid(20); }
    ";
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    assert_eq!(lowered.artifact.functions.len(), 3, "reachable closure must include Main, Mid, and Inner");
    let main = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Main#syntax_"))
        .expect("Main artifact function");
    let mid = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Mid#syntax_"))
        .expect("Mid artifact function");
    assert!(main.function.display().to_string().contains("call"), "Main must call Mid");
    assert!(mid.function.display().to_string().matches("call").count() >= 2, "Mid must nest two Inner calls");
}

#[test]
fn unsupported_lambda_fails_closed_without_legacy_fallback() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        i32 Main() {
            i32 outer = 1;
            let add = (i32 inner) => outer + inner;
            return outer;
        }
    ";
    let assembly = parse_production_units(project.path(), &[("Lambda.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();
    assert_unsupported_closed_failure(assembly, target, isa.as_ref(), &["Lambda.bd", "MissingRuleOrFact"]);
}

#[test]
fn parsed_project_inline_method_reaches_verified_clif_through_production_entrypoint() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        type Point { i32 x, i32 Ping() { return 7; } }
        i32 Main() { return Point { x: 1 }.Ping(); }
    ";
    let assembly = parse_production_units(project.path(), &[("Method.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    assert!(
        lowered.artifact.functions.len() >= 2,
        "reachable closure must include Main and the inline Point.Ping method"
    );
    let main = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Main#syntax_"))
        .expect("Main artifact function");
    assert!(
        main.function.display().to_string().contains("call"),
        "Main must call the inline method through syntax ISLE"
    );
}

#[test]
fn parsed_project_capturing_lambda_keeps_generation_safe_capture_facts_and_fails_closed() {
    let project = tempfile::tempdir().expect("project directory");
    let source_path = project.path().join("Capture.bd");
    let source = "
        i32 Main() {
            i32 outer = 1;
            let apply = (i32 inner) => outer + inner;
            return outer;
        }
    ";
    let assembly = parse_production_units(project.path(), &[("Capture.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    with_db(|db| {
        let generation = SyntaxGenerationId(1);
        let unit = SourceUnitId::new(db, source_path.clone());
        let typed = beskid_queries::build_typed_program(
            db,
            beskid_queries::ProjectSession::new(
                db,
                project.path().to_path_buf(),
                source_path.clone(),
                "App".into(),
                "lock".into(),
            ),
            generation,
            Arc::clone(&assembly),
        )
        .expect("typed capture program");
        assert!(
            typed.runtime_intrinsic_capability.is_none(),
            "ordinary parsed projects must not mint trusted runtime intrinsic authority"
        );
        let root = AstNodeKey { unit, generation, node: AstNodeId(0) };
        let mut pending = vec![root];
        let mut lambda = None;
        while let Some(key) = pending.pop() {
            if matches!(node_kind(db, key), Ok(Some(beskid_queries::IndexedNodeKind::LambdaExpression))) {
                lambda = Some(key);
                break;
            }
            if let Ok(Some(children)) = child_nodes(db, key) {
                pending.extend(children.iter().copied());
            }
        }
        let lambda = lambda.expect("capturing lambda source node");
        let environment =
            closure_environment(db, lambda).expect("capture fact query").expect("generation-safe capture environment");
        assert_eq!(environment.captures.len(), 1, "outer parameter is captured");
        assert_eq!(environment.parameters.len(), 1, "inner lambda parameter");
    });

    assert_unsupported_closed_failure(assembly, target, isa.as_ref(), &["Capture.bd", "MissingRuleOrFact"]);
}

#[test]
fn canonical_runtime_production_path_lowers_trusted_intrinsics_to_verified_clif() {
    let (target, isa) = x86_64_target_and_isa();
    let expected_exports = AbiManifestV5::canonical_runtime(target.clone())
        .exports
        .into_iter()
        .map(|entry| entry.symbol)
        .collect::<BTreeSet<_>>();
    let artifact = with_db(|db| lower_canonical_runtime_prepared_syntax(db, target, isa.as_ref()))
        .expect("canonical runtime lowers through TypedProgram → CodegenInput → ISLE");
    assert!(!artifact.functions.is_empty(), "canonical Bootstrap must emit at least one verified function");
    assert!(
        artifact.exports.iter().any(|export| export.exported_symbol == "beskid_rt_v5_fiber_spawn_with_cancel_slot"),
        "canonical runtime lowering must retain the Scheduler-owned fiber spawn ABI export",
    );
    let actual_exports = artifact.exports.iter().map(|export| export.exported_symbol.clone()).collect::<BTreeSet<_>>();
    assert_eq!(actual_exports, expected_exports, "canonical lowering publishes exactly the ABI manifest surface");
    assert!(
        !actual_exports.contains("gc_alloc"),
        "generic runtime helper exports must not become ABI roots without a manifest declaration",
    );
    for function in &artifact.functions {
        verify_function(&function.function, isa.flags())
            .unwrap_or_else(|error| panic!("stock CLIF verifier rejected {}: {error}", function.name));
    }
    assert!(
        artifact.functions.iter().any(|function| {
            let clif = function.function.display().to_string();
            clif.contains("iconst") || clif.contains("load") || clif.contains("store")
        }),
        "canonical runtime helpers must emit real CLIF bodies"
    );
}

#[test]
fn production_path_never_constructs_hir_or_lowerable() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        i32 Helper(i32 value) { return value + 1; }
        i32 Main() {
            if Helper(1) > 0 { return Helper(2); }
            return 0;
        }
    ";
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    // Production boundary is SyntaxProgramAssembly-only: no HirProgram / Lowerable parameter.
    assert_eq!(
        std::any::type_name_of_val(assembly.as_ref()),
        "beskid_analysis::projects::assembly::SyntaxProgramAssembly"
    );
    let (target, isa) = x86_64_target_and_isa();
    let lowered = lower_verified_entrypoint(Arc::clone(&assembly), target.clone(), isa.as_ref());
    assert!(lowered.artifact.functions.len() >= 2, "direct-call closure through syntax ISLE");

    let public_exports = include_str!("../src/lib.rs");
    assert!(
        public_exports.contains("lower_syntax_assembly_entrypoint"),
        "production codegen must expose the syntax-assembly lowering boundary"
    );
    assert!(
        public_exports.contains("lower_prepared_syntax_entrypoint"),
        "production codegen must expose the prepared-syntax lowering boundary"
    );
}

#[test]
fn public_codegen_surface_names_canonical_syntax_lowering_authority() {
    let public_exports = include_str!("../src/lib.rs");
    let public_prepared_syntax = include_str!("../src/prepared_syntax.rs");
    assert!(
        public_exports.contains("lower_prepared_syntax_module"),
        "module hosts must use the canonical prepared-syntax lowering boundary"
    );
    assert!(
        public_prepared_syntax.contains("CodegenInput::new"),
        "prepared-syntax lowering must construct CodegenInput before emitting ISLE"
    );
    assert!(
        public_prepared_syntax.contains("lower_syntax_program"),
        "prepared-syntax lowering must emit through the syntax ISLE authority"
    );
}
