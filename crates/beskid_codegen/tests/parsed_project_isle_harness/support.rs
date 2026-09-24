//! Shared parsing, target, and verified-lowering helpers for the harness.

use super::*;

pub(super) fn parse_production_units(root: &std::path::Path, units: &[(&str, &str, &str)]) -> Arc<ProgramAssembly> {
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
            origin_path: path.clone(),
            path,
            source: (*source).into(),
            program,
        });
    }
    Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.to_path_buf() },
            dependencies: Vec::new(),
        },
        Arc::new(source_units),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        SyntaxGenerationId(0),
    ))
}

pub(super) fn x86_64_target_and_isa() -> (TargetMetadata, std::sync::Arc<dyn cranelift_codegen::isa::TargetIsa>) {
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("Linux x86_64 ABI target");
    let settings = beskid_codegen::cranelift_host::production_isa_settings_builder().expect("production ISA settings");
    let isa =
        isa::lookup_by_name("x86_64").expect("x86 ISA").finish(settings::Flags::new(settings)).expect("finish ISA");
    (target, isa)
}

pub(super) fn lower_verified_entrypoint(
    assembly: Arc<ProgramAssembly>,
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

pub(super) fn assert_unsupported_closed_failure(
    assembly: Arc<ProgramAssembly>,
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
    assert!(rendered.contains("MissingRuleOrFact") || rendered.contains("spawn legality rejected"), "{rendered}");
    for fragment in expected_site_fragments {
        assert!(rendered.contains(fragment), "expected {fragment:?} in {rendered}");
    }
}
