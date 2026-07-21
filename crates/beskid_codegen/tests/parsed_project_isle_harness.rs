use std::sync::Arc;

use beskid_abi::abi_v5::TargetMetadata;
use beskid_analysis::services::{
    FrontEndOptions, resolved_input_from_plan, synthetic_compile_plan_for_source,
};
use beskid_analysis::{
    projects::{
        AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, RootEntry, SourceUnit,
        SyntaxProgramAssembly,
    },
    services::parse_program_with_source_name,
};
use beskid_codegen::lower_syntax_assembly_entrypoint;
use beskid_codegen::lowering::lower_program;
use beskid_queries::{compile_front_end_from_resolved_input, with_db};
use cranelift_codegen::{isa, settings, verify_function};

const RETIRED_HIR_PATH_MARKER: &str = beskid_codegen::RETIRED_HIR_LOWERING_PATH;

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

fn parse_production_units(
    root: &std::path::Path,
    units: &[(&str, &str, &str)],
) -> Arc<SyntaxProgramAssembly> {
    let mut source_units = Vec::with_capacity(units.len());
    for (relative_path, logical_name, source) in units {
        let path = root.join(relative_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("unit parent directory");
        }
        std::fs::write(&path, source).expect("write project source");
        let program =
            parse_program_with_source_name(path.to_str().expect("UTF-8 source path"), source)
                .expect("production source parse");
        source_units.push(SourceUnit {
            logical_name: (*logical_name).into(),
            path,
            source: (*source).into(),
            program,
        });
    }
    Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: root.to_path_buf(),
            },
            dependencies: Vec::new(),
        },
        Arc::new(source_units),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ))
}

fn x86_64_target_and_isa() -> (
    TargetMetadata,
    std::sync::Arc<dyn cranelift_codegen::isa::TargetIsa>,
) {
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
        verify_function(&function.function, isa.flags()).unwrap_or_else(|error| {
            panic!("stock CLIF verifier rejected {}: {error}", function.name)
        });
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
        assert!(
            rendered.contains(fragment),
            "expected {fragment:?} in {rendered}"
        );
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
    assert_eq!(
        lowered.artifact.functions.len(),
        2,
        "reachable direct-call closure"
    );

    let unsupported_source = "
        i32 Main() {
            i32 outer = 1;
            let task = spawn ((i32 inner) => outer + inner);
            return outer;
        }
    ";
    let unsupported = parse_production_units(
        project.path(),
        &[("Unsupported.bd", "Main", unsupported_source)],
    );
    assert_unsupported_closed_failure(
        unsupported,
        target,
        isa.as_ref(),
        &["Unsupported.bd", "Block@"],
    );
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
    assert_eq!(
        lowered.artifact.functions.len(),
        3,
        "Entry, Main, and the syntax-owned spawn trampoline"
    );
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
    assert!(
        main_clif.contains("beskid_rt_v5_fiber_spawn_with_cancel_slot"),
        "{main_clif}"
    );
    assert!(!main_clif.contains("interop_dispatch_"), "{main_clif}");
    assert!(main_clif.contains("func_addr"), "{main_clif}");
    assert!(
        trampoline_clif.contains("Entry#syntax_"),
        "{trampoline_clif}"
    );
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
    assert_eq!(
        lowered.artifact.functions.len(),
        3,
        "Main, the syntax-owned lambda entry, and its spawn trampoline"
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
    assert!(
        main_clif.contains("beskid_rt_v5_fiber_spawn_with_cancel_slot"),
        "{main_clif}"
    );
    assert!(!main_clif.contains("interop_dispatch_"), "{main_clif}");
    assert!(main_clif.contains("func_addr"), "{main_clif}");
    assert!(lambda_clif.contains("iconst.i64 7"), "{lambda_clif}");
    assert!(
        trampoline_clif.contains("__beskid_spawn_lambda_syntax_"),
        "{trampoline_clif}"
    );
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
    let assembly = parse_production_units(
        project.path(),
        &[
            ("Main.bd", "Main", main_source),
            ("Util.bd", "Util", util_source),
        ],
    );
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    assert!(
        lowered.artifact.functions.len() >= 2,
        "reachable closure must include Main and imported Util.Double"
    );
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
    assert!(
        clif.matches("jump").count() >= 2,
        "expected loop transfer jumps: {clif}"
    );
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
    assert!(
        clif.contains("iadd"),
        "expected accumulator addition: {clif}"
    );
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
    assert_eq!(
        lowered.artifact.functions.len(),
        3,
        "reachable closure must include Main, Mid, and Inner"
    );
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
    assert!(
        main.function.display().to_string().contains("call"),
        "Main must call Mid"
    );
    assert!(
        mid.function.display().to_string().matches("call").count() >= 2,
        "Mid must nest two Inner calls"
    );
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
    assert_unsupported_closed_failure(
        assembly,
        target,
        isa.as_ref(),
        &["Lambda.bd", "MissingRuleOrFact"],
    );
}

#[test]
fn production_path_never_constructs_hir_or_lowerable() {
    let project = tempfile::tempdir().expect("project directory");
    let path = project.path().join("Main.bd");
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
    assert!(
        lowered.artifact.functions.len() >= 2,
        "direct-call closure through syntax ISLE"
    );

    let plan = synthetic_compile_plan_for_source(&path);
    let resolved = resolved_input_from_plan(path, source.to_string(), plan, None, None);
    let front = compile_front_end_from_resolved_input(
        &resolved,
        FrontEndOptions {
            with_semantic_diagnostics: false,
            ..Default::default()
        },
        None,
    )
    .expect("front-end for Lowerable rejection probe");
    match lower_program(&front.hir, &front.resolution, &front.typed) {
        Ok(_) => panic!("lower_program must not construct a Lowerable artifact"),
        Err(errors) => {
            let message = errors
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ");
            assert!(message.contains(RETIRED_HIR_PATH_MARKER), "{message}");
            assert!(message.contains("lower_syntax_"), "{message}");
        }
    }
}

#[test]
fn remaining_hir_driver_is_rejected_without_fallback() {
    let project = tempfile::tempdir().expect("project directory");
    let path = project.path().join("Main.bd");
    let source = "i32 Main() { return 1; }";
    std::fs::write(&path, source).expect("write source");

    let plan = synthetic_compile_plan_for_source(&path);
    let resolved = resolved_input_from_plan(path, source.to_string(), plan, None, None);
    let front = compile_front_end_from_resolved_input(
        &resolved,
        FrontEndOptions {
            with_semantic_diagnostics: false,
            ..Default::default()
        },
        None,
    )
    .expect("front-end for rejection probe");
    match lower_program(&front.hir, &front.resolution, &front.typed) {
        Ok(_) => panic!("lower_program must reject the retired HIR path"),
        Err(errors) => {
            let message = errors
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ");
            assert!(message.contains(RETIRED_HIR_PATH_MARKER), "{message}");
            assert!(message.contains("lower_syntax_"), "{message}");
        }
    }
}
