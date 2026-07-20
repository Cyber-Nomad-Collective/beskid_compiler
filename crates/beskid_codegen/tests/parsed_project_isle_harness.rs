use std::sync::Arc;

use beskid_abi::abi_v5::TargetMetadata;
use beskid_analysis::{
    projects::{
        AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, RootEntry, SourceUnit,
        SyntaxProgramAssembly,
    },
    services::parse_program_with_source_name,
};
use beskid_analysis::services::{
    FrontEndOptions, resolved_input_from_plan, synthetic_compile_plan_for_source,
};
use beskid_codegen::lower_syntax_assembly_entrypoint;
use beskid_codegen::lowering::lower_program;
use beskid_codegen::lower_source;
use beskid_queries::{compile_front_end_from_resolved_input, with_db};
use cranelift_codegen::{isa, settings, verify_function};

const RETIRED_HIR_PATH_MARKER: &str = beskid_codegen::RETIRED_HIR_LOWERING_PATH;

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
        let program = parse_program_with_source_name(path.to_str().expect("UTF-8 source path"), source)
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
    let lowered = with_db(|db| {
        lower_syntax_assembly_entrypoint(db, assembly, "Main", target.clone(), isa.as_ref())
    })
    .expect("parsed project lowers through syntax facts and generated ISLE");

    assert_eq!(
        lowered.artifact.functions.len(),
        2,
        "reachable direct-call closure"
    );
    assert!(lowered.symbol.starts_with("Main#syntax_"));
    for function in &lowered.artifact.functions {
        verify_function(&function.function, isa.flags()).unwrap_or_else(|error| {
            panic!("stock CLIF verifier rejected {}: {error}", function.name)
        });
    }

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
    let unsupported_result = with_db(|db| {
        lower_syntax_assembly_entrypoint(db, unsupported, "Main", target, isa.as_ref())
    });
    let error = match unsupported_result {
        Ok(_) => panic!("unsupported spawn must not fall back to legacy codegen"),
        Err(error) => error,
    };
    let rendered = error.to_string();
    assert!(rendered.contains("MissingRuleOrFact"), "{rendered}");
    assert!(rendered.contains("Unsupported.bd"), "{rendered}");
    assert!(rendered.contains("Block@"), "{rendered}");
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

    let lowered = with_db(|db| {
        lower_syntax_assembly_entrypoint(db, assembly, "Main", target, isa.as_ref())
    })
    .expect("multi-unit parsed project lowers through CodegenInput and ISLE");

    assert!(
        lowered.artifact.functions.len() >= 2,
        "reachable closure must include Main and imported Util.Double"
    );
    assert!(lowered.symbol.starts_with("Main#syntax_"));
    for function in &lowered.artifact.functions {
        verify_function(&function.function, isa.flags()).unwrap_or_else(|error| {
            panic!("stock CLIF verifier rejected {}: {error}", function.name)
        });
    }
}

#[test]
fn hir_and_lowerable_entrypoints_are_rejected_without_fallback() {
    let project = tempfile::tempdir().expect("project directory");
    let path = project.path().join("Main.bd");
    let source = "i32 Main() { return 1; }";
    std::fs::write(&path, source).expect("write source");

    match lower_source(&path, source, false) {
        Ok(_) => panic!("lower_source must reject the retired HIR path"),
        Err(error) => {
            let message = error.to_string();
            assert!(
                message.contains(RETIRED_HIR_PATH_MARKER),
                "{message}"
            );
            assert!(message.contains("CodegenInput"), "{message}");
        }
    }

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
            assert!(
                message.contains(RETIRED_HIR_PATH_MARKER),
                "{message}"
            );
            assert!(message.contains("lower_syntax_"), "{message}");
        }
    }
}
