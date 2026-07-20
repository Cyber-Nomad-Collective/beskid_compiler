use std::sync::Arc;

use beskid_abi::abi_v5::TargetMetadata;
use beskid_analysis::{
    projects::{
        AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, RootEntry, SourceUnit,
        SyntaxProgramAssembly,
    },
    services::parse_program_with_source_name,
};
use beskid_codegen::lower_syntax_assembly_entrypoint;
use beskid_queries::with_db;
use cranelift_codegen::{isa, settings, verify_function};

fn parse_production_project(path: &std::path::Path, source: &str) -> Arc<SyntaxProgramAssembly> {
    std::fs::write(path, source).expect("write project source");
    let program = parse_program_with_source_name(path.to_str().expect("UTF-8 source path"), source)
        .expect("production source parse");
    let root = path.parent().expect("project root").to_path_buf();
    Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: root,
            },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: "Main".into(),
            path: path.to_path_buf(),
            source: source.into(),
            program,
        }]),
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
    let path = project.path().join("Main.bd");
    let source = "
        type Pair { i32 left, i32 right }
        i32 Add(i32 left, i32 right) { return left + right; }
        i32 Main() {
            Pair pair = Pair { left: 19, right: 23 };
            if pair.left < pair.right { return Add(pair.left, pair.right); }
            return 0;
        }
    ";
    let assembly = parse_production_project(&path, source);
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

    let unsupported_path = project.path().join("Unsupported.bd");
    let unsupported_source = "
        i32 Main() {
            i32 outer = 1;
            let task = spawn ((i32 inner) => outer + inner);
            return outer;
        }
    ";
    let unsupported = parse_production_project(&unsupported_path, unsupported_source);
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
