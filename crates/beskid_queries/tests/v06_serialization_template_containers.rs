//! A specialized serialization template cannot carry a Foundation container field.
//!
//! The Mod proves a `ContainerSite` only through the semantic closure of a
//! concrete target. Generic templates have no such proof, so the host
//! specialization gate rejects List, Map and Option fields after substitution,
//! including a container that arrives through a type parameter.
use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};
use beskid_abi::runtime_source::canonical_corelib_service_capability;
use beskid_analysis::mod_host::{ModSemanticAuthority, ModSemanticFieldType, ModSemanticShapeBody};
use beskid_analysis::projects::{
    AssemblyOptions, CompilePlan, ResolvedDependencyProject, Target, TargetKind, assemble_program_with_materializer,
};
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{
    AstNodeKey, BeskidDatabase, ModSemanticQueryAuthority, ProjectSession, SourceUnitId,
    build_typed_program_with_corelib_services, project_session_for_planned_syntax_assembly,
};
use std::sync::Arc;

const SOURCE: &str = r#"
use Core.Collections.List;
use Core.Optional;
pub type Bag<T> { pub Option<T> Item, }
pub type Many<T> { pub List<T>[] Groups, }
pub type Box<T> { pub T Value, }
pub type Plain<T> { pub T Value, pub i32[] Counts, }
pub type Uses { pub Bag<i32> A, pub Many<string> B, pub Box<Option<i32>> C, pub Plain<string> D, }
pub unit Main() { return; }
"#;

struct Fixture {
    _root: tempfile::TempDir,
    db: BeskidDatabase,
    project: ProjectSession,
    assembly: Arc<beskid_analysis::projects::ProgramAssembly>,
}

fn fixture() -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let source_root = root.path().canonicalize().unwrap().join("Src");
    std::fs::create_dir_all(&source_root).unwrap();
    let foundation = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corelib/packages/foundation")
        .canonicalize()
        .unwrap();
    let path = source_root.join("Main.bd");
    std::fs::write(&path, SOURCE).unwrap();
    let plan = CompilePlan {
        project_root: source_root.parent().unwrap().to_owned(),
        manifest_path: source_root.parent().unwrap().join("Host.bproj"),
        project_name: "Host".into(),
        source_root,
        target: Target { name: "Main".into(), kind: TargetKind::Lib, entry: Some("Main.bd".into()) },
        dependency_projects: vec![ResolvedDependencyProject {
            dependency_name: "corelib_foundation".into(),
            manifest_path: foundation.join("corelib_foundation.bproj"),
            project_root: foundation.clone(),
            project_name: "corelib_foundation".into(),
            source_root: foundation.join("src"),
        }],
        unresolved_dependencies: vec![],
        has_std_dependency: false,
    };
    let assembly = Arc::new(
        assemble_program_with_materializer(&plan, None, &path, None, &AssemblyOptions::default(), None, None).unwrap(),
    );
    let mut db = BeskidDatabase::default();
    let project =
        project_session_for_planned_syntax_assembly(&mut db, &assembly, &plan, &path, "template-containers".into())
            .unwrap();
    let manifest = AbiManifestV5::canonical_runtime(
        TargetMetadata::supported().into_iter().find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu").unwrap(),
    );
    build_typed_program_with_corelib_services(
        &mut db,
        project,
        assembly.generation,
        assembly.clone(),
        canonical_corelib_service_capability(&manifest).unwrap(),
    )
    .unwrap();
    Fixture { _root: root, db, project, assembly }
}

#[test]
fn specialized_template_rejects_foundation_container_fields() {
    let fixture = fixture();
    let authority =
        ModSemanticQueryAuthority::for_registered_assembly(&fixture.db, fixture.project, &fixture.assembly).unwrap();
    let unit = SourceUnitId::new(&fixture.db, fixture.assembly.entry_unit().path.clone());
    let uses = fixture
        .assembly
        .entry_syntax_index()
        .ids_of_kind(NodeKind::TypeDefinition)
        .map(|node| AstNodeKey { unit, generation: fixture.assembly.generation, node })
        .map(|key| authority.type_shape(authority.resolve_type(key).unwrap_or_else(|_| panic!("type"))))
        .find_map(|shape| shape.ok().filter(|shape| shape.name == "Uses"))
        .expect("Uses fixture");
    let ModSemanticShapeBody::Record { fields } = uses.body else { panic!("record fixture") };
    let applied = |index: usize| {
        let ModSemanticFieldType::Nominal(handle) = fields[index].ty else { panic!("applied template field") };
        handle
    };
    // Declared Option<T>, a List<T> reached through an array, and an Option
    // that arrives only through the type parameter all fail the same way.
    for index in 0..3 {
        let error = authority.serialization_template_shape(applied(index)).unwrap_err().to_string();
        assert!(error.contains("SerializationTemplateContainer"), "field {index}: {error}");
    }
    // A template without container fields is not rejected by the container rule.
    if let Err(error) = authority.serialization_template_shape(applied(3)) {
        assert!(!error.to_string().contains("SerializationTemplateContainer"), "{error}");
    }
}
