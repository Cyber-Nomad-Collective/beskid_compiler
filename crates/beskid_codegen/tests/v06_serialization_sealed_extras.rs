//! Physical source layout is usable only at the exact sealed backing constructor.
#[path = "isle_adapter/support.rs"] mod support;
use support::*;
use beskid_queries::{aggregate_literal_declaration, runtime_managed_opaque_kind, RuntimeManagedOpaqueKind, TypedProgram};

fn fixture(tamper: bool) -> (BeskidDatabase, TypedProgram, AstNodeKey, AstNodeKey) {
    let directory = tempfile::tempdir().unwrap();
    let mut units = Vec::new();
    for canonical in canonical_corelib_service_sources() {
        let identity = beskid_abi::runtime_source::corelib_service_source_identity(&canonical.logical_path).unwrap();
        let mut program = parse_program_with_source_name("canonical", &canonical.source).unwrap();
        if tamper && canonical.logical_path == beskid_abi::runtime_source::CANONICAL_SERIALIZATION_DESCRIPTORS_SOURCE_PATH {
            let added = parse_program_with_source_name("tamper", "unit Unissued() { return; }").unwrap();
            program.node.items.extend(added.node.items);
        }
        units.push(SourceUnit { logical_name: canonical.logical_path, origin_path: identity.declared_path,
            path: identity.canonical_path, source: canonical.source, program });
    }
    let metadata = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corelib/packages/serialization/src/Core/Serialization/Metadata.bd").canonicalize().unwrap();
    let text = std::fs::read_to_string(&metadata).unwrap();
    units.push(SourceUnit { logical_name: "Core.Serialization.Metadata".into(), origin_path: metadata.clone(), path: metadata,
        program: parse_program_with_source_name("Metadata", &text).unwrap(), source: text });
    let user = directory.path().join("User.bd");
    let text = "use Core.Serialization.Descriptors; pub unit Main() { ExtrasBinding forged = ExtrasBinding {}; return; }";
    std::fs::write(&user, text).unwrap();
    units.push(SourceUnit { logical_name: "User".into(), origin_path: user.clone(), path: user.clone(), source: text.into(),
        program: parse_program_with_source_name("User", text).unwrap() });
    let entry = units.len()-1;
    let generation = SyntaxGenerationId(331);
    let assembly = Arc::new(ProgramAssembly::new(EffectiveCompilationRoots {
        host: RootEntry { dependency_name: None, source_root: directory.path().into() }, dependencies: vec![],
    }, Arc::new(units), entry, AssemblyDiscovery::ImportClosure, Arc::new(ModuleIndex::empty()), false, generation));
    let descriptor_index = assembly.units.iter().position(|unit| unit.logical_name ==
        beskid_abi::runtime_source::CANONICAL_SERIALIZATION_DESCRIPTORS_SOURCE_PATH).unwrap();
    let mut db = BeskidDatabase::default();
    let project = ProjectSession::new(&db, directory.path().into(), user.clone(), "sealed-extras".into(), "canonical".into());
    let target = TargetMetadata::supported().into_iter().find(|target| target.triple.as_str()=="x86_64-unknown-linux-gnu").unwrap();
    let capability = canonical_corelib_service_capability(&AbiManifestV5::canonical_runtime(target)).unwrap();
    let typed = build_typed_program_with_corelib_services(&mut db, project, generation, assembly.clone(), capability).unwrap();
    let constructor = AstNodeKey { unit: SourceUnitId::new(&db, assembly.units[descriptor_index].path.clone()), generation,
        node: assembly.syntax_indexes[descriptor_index].ids_of_kind(NodeKind::StructLiteralExpression).next().unwrap() };
    let forged = AstNodeKey { unit: SourceUnitId::new(&db, user), generation,
        node: assembly.entry_syntax_index().ids_of_kind(NodeKind::StructLiteralExpression).next().unwrap() };
    (db, typed, constructor, forged)
}
#[test]
fn exact_private_constructor_has_source_layout_but_user_literal_is_denied() {
    let (db, _typed, constructor, forged) = fixture(false);
    let declaration = aggregate_literal_declaration(&db, constructor).unwrap().unwrap();
    assert_eq!(runtime_managed_opaque_kind(&db,declaration),Some(RuntimeManagedOpaqueKind::SerializationExtras));
    assert!(aggregate_literal_declaration(&db,forged).is_err());
}
#[test]
fn modified_canonical_tree_cannot_issue_sealed_backing_constructor() {
    let (db, _typed, constructor, forged) = fixture(true);
    assert!(aggregate_literal_declaration(&db,constructor).is_err());
    assert!(aggregate_literal_declaration(&db,forged).is_err());
}
