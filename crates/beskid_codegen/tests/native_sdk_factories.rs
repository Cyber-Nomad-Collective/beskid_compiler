//! Native requests are constructed by real typed SDK source, never host struct casts.
use beskid_abi::abi_v5::TargetMetadata;
use beskid_analysis::services::{FrontEndOptions, resolved_input_from_plan};
use beskid_codegen::lower_prepared_syntax_selected_callables;
use beskid_queries::{compile_front_end_from_resolved_input, with_db};
use cranelift_codegen::{isa, settings};
use std::path::PathBuf;

#[test]
fn generated_sdk_type_mirror_parses_without_recovery() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corelib/packages/compiler-sdk/src/Beskid/Syntax/Nodes/Type.bd");
    let source = std::fs::read_to_string(&path).unwrap();
    let parsed =
        beskid_analysis::services::parse_program_with_source_name_and_diagnostics(path.to_str().unwrap(), &source)
            .expect("canonical generated SDK types must parse without repair");
    assert!(
        !parsed.recovered && parsed.diagnostics.is_empty(),
        "SDK parser repaired generated source: {:?}",
        parsed.diagnostics
    );
}

#[test]
fn every_generated_sdk_source_parses_without_recovery() {
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corelib/packages/compiler-sdk/src/Beskid/Syntax/Nodes");
    let mut paths = std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "bd"))
        .collect::<Vec<_>>();
    paths.sort();
    assert!(paths.len() >= 99, "generated syntax inventory is incomplete");
    for path in paths {
        let source = std::fs::read_to_string(&path).unwrap();
        let parsed =
            beskid_analysis::services::parse_program_with_source_name_and_diagnostics(path.to_str().unwrap(), &source)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        assert!(
            !parsed.recovered && parsed.diagnostics.is_empty(),
            "{} required parser repair: {:?}",
            path.display(),
            parsed.diagnostics
        );
    }
}

#[test]
fn canonical_sdk_request_factories_lower_with_compiler_owned_descriptors() {
    let package = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corelib/packages/compiler-sdk");
    // Compile the real SDK package graph. Mutating an orphan's default Core plan
    // leaves its dependencies pointing at a different materialized SDK package.
    let plan = beskid_analysis::projects::build_compile_plan(
        &package.join("corelib_compiler_sdk.bproj"),
        Some("CompilerSdkLib"),
    )
    .expect("canonical SDK package compile plan");
    // A replayed lock moves the host root to its materialized copy. The entry must be the
    // unit that root registers; a second copy of the request source would be a second
    // canonical constructor authority.
    let host_root = beskid_analysis::projects::effective_roots_for_plan(&plan, None).host.source_root;
    let path = host_root.join("Beskid/Compiler/NativeRequests.bd");
    let source = std::fs::read_to_string(&path).unwrap();
    let options =
        beskid_analysis::projects::assembly_options_for_prepare(&plan, FrontEndOptions::default().assembly_discovery);
    let assembly = beskid_analysis::projects::assemble_program_with_materializer(
        &plan,
        None,
        &path,
        Some(&source),
        &options,
        None,
        None,
    )
    .expect("canonical SDK production assembly before type checking");
    let request_units = assembly
        .units
        .iter()
        .filter(|unit| unit.path.file_name().is_some_and(|name| name == "NativeRequests.bd"))
        .count();
    assert_eq!(request_units, 1, "the SDK request source must register exactly once");
    let canonical_compilation = std::fs::read_to_string(package.join("src/Beskid/Compiler/Compilation.bd")).unwrap();
    assert!(assembly.units.iter().any(|unit| unit.path.ends_with("Beskid/Syntax/Nodes/NodeRef.bd")),
        "qualified Compilation.entryRoot dependency NodeRef is absent from production assembly: {:?}",
        assembly.units.iter().map(|unit| &unit.path).collect::<Vec<_>>());
    let compilation_units = assembly
        .units
        .iter()
        .filter(|unit| unit.path.file_name().is_some_and(|name| name == "Compilation.bd"))
        .collect::<Vec<_>>();
    assert!(
        !compilation_units.is_empty(),
        "registered SDK graph has no Compilation source; units: {:?}",
        assembly.units.iter().map(|unit| &unit.path).collect::<Vec<_>>()
    );
    for unit in &compilation_units {
        assert_eq!(
            unit.source,
            canonical_compilation,
            "registered Compilation bytes differ at {}",
            unit.path.display()
        );
        let projection = serde_json::to_value(&unit.program).unwrap();
        assert!(
            projection.to_string().contains("entryRoot"),
            "current Compilation parsed AST omitted entryRoot at {}: {projection}",
            unit.path.display()
        );
    }
    for unit in &compilation_units {
        let resolution = assembly.module_index
            .resolve_entry_program(&assembly.entry_unit().program, Some(&path), &assembly)
            .expect("resolve actual entry declaration closure");
        let surface = beskid_analysis::types::build_unit_type_surface(&unit.program, &resolution, &unit.path);
        let owners = resolution.items.iter().filter(|item|
            item.kind == beskid_analysis::resolve::ItemKind::Type && item.name == "Compilation")
            .collect::<Vec<_>>();
        assert!(owners.iter().any(|owner| surface.struct_fields_ordered.get(&owner.id)
            .is_some_and(|fields| fields.iter().any(|(name, _)| name == "entryRoot"))),
            "actual Compilation dependency surface dropped entryRoot; owners={owners:?}; fields={:?}; modules={:?}",
            surface.struct_fields_ordered, resolution.items.iter().filter(|item| item.name == "NodeRef").collect::<Vec<_>>());
    }
    let resolved = resolved_input_from_plan(path, source, plan, None, Some(assembly));
    let front = compile_front_end_from_resolved_input(
        &resolved,
        FrontEndOptions { with_semantic_diagnostics: false, ..Default::default() },
        None,
    )
    .unwrap();
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .unwrap();
    let isa = isa::lookup_by_name("x86_64").unwrap().finish(settings::Flags::new(settings::builder())).unwrap();
    let prepared = with_db(|db| {
        let assembly = front.syntax_assembly();
        let mut constructor_keys = Vec::new();
        for unit in assembly.units.iter() {
            let root = beskid_queries::AstNodeKey {
                unit: beskid_queries::SourceUnitId::new(db, unit.path.clone()),
                generation: assembly.generation,
                node: beskid_queries::AstNodeId(0),
            };
            let mut pending = vec![root];
            while let Some(key) = pending.pop() {
                if beskid_queries::node_kind(db, key)? == Some(beskid_queries::IndexedNodeKind::FunctionDefinition)
                    && beskid_queries::native_mod_request_constructor(db, key)?.is_some()
                {
                    constructor_keys.push(key);
                }
                if let Some(children) = beskid_queries::child_nodes(db, key)? {
                    pending.extend(children.iter().copied());
                }
            }
        }
        assert_eq!(constructor_keys.len(), 12, "all canonical request constructor entries required");
        assert!(lower_prepared_syntax_selected_callables(db, &front, &[], target.clone(), isa.as_ref()).is_err());
        let stale_selection = beskid_queries::AstNodeKey {
            generation: beskid_queries::SyntaxGenerationId(assembly.generation.0 + 1),
            ..constructor_keys[0]
        };
        assert!(lower_prepared_syntax_selected_callables(db, &front, &[stale_selection], target.clone(), isa.as_ref()).is_err());
        let prepared = lower_prepared_syntax_selected_callables(db, &front, &constructor_keys, target, isa.as_ref())?;
        let mut issued = std::collections::HashSet::new();
        for callable in prepared.callables() {
            if let Some(factory) = beskid_queries::native_mod_request_constructor(db, callable.key())? {
                assert!(issued.insert(factory.role()), "factory role must have one current source declaration");
                assert_eq!(factory.key(), callable.key());
                assert_eq!(factory.signature(), callable.signature());
                if factory.role() == beskid_queries::NativeModRequestFactory::Compilation {
                    assert_eq!(factory.signature().parameters.len(), 7);
                    let signature = beskid_queries::native_mod_transport_signature(db, callable.key())?
                        .expect("compilation factory must retain exact source types");
                    let root = signature.parameters().last().expect("compilation entry root missing");
                    assert!(
                        matches!(root, beskid_queries::NativeModTransportType::Nominal { .. }),
                        "root must have nominal source identity"
                    );
                    let nominal = beskid_queries::native_mod_transport_nominal(db, callable.key(), root)?
                        .expect("entry root source declaration missing");
                    assert_eq!(nominal.name(), "NodeRef");
                }
                let stale = beskid_queries::AstNodeKey {
                    generation: beskid_queries::SyntaxGenerationId(factory.key().generation.0 + 1),
                    ..factory.key()
                };
                assert!(beskid_queries::native_mod_request_constructor(db, stale)?.is_none());
            }
        }
        assert_eq!(issued.len(), 12, "all twelve source constructors must be issued without name guessing");
        Ok::<_, anyhow::Error>(prepared)
    })
    .unwrap();
    for name in [
        "CompilationValue",
        "MemberValue",
        "WorkspaceValue",
        "RegistrationValue",
        "PackageValue",
        "CatalogValue",
        "CollectValue",
        "TargetsValue",
        "GenerationValue",
        "AnalysisValue",
        "AttributeValue",
        "AttributeDeclarationsValue",
    ] {
        assert!(
            prepared
                .callables()
                .iter()
                .any(|callable| callable.internal_symbol().starts_with(&format!("{name}#syntax_"))),
            "missing typed factory {name}"
        );
    }
    assert!(prepared.artifact().aggregate_static_plans.len() >= 12);
    assert!(
        prepared
            .artifact()
            .aggregate_static_plans
            .iter()
            .all(|plan| !plan.descriptor_symbol.is_empty() && !plan.pointer_map_offsets.is_empty()),
        "every request constructor owns a compiler-emitted traced descriptor"
    );
}
