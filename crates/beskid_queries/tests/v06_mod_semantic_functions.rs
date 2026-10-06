//! Mod function resolution and the serialization gate use the registered assembly's canonical
//! name resolution and host shape gate, never a Mod-side spelling search.
use beskid_analysis::mod_host::{
    ModSemanticAuthority, ModSemanticFieldType, ModSemanticShapeBody, ModSyntaxAuthority, ModSyntaxNodeRef,
    ModSyntaxRequest, ModSyntaxResponse,
};
use beskid_analysis::projects::{
    AssemblyOptions, CompilePlan, ProgramAssembly, ResolvedDependencyProject, Target, TargetKind,
    assemble_program_with_materializer,
};
use beskid_analysis::syntax::PrimitiveType;
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{
    AstNodeKey, BeskidDatabase, ModSemanticQueryAuthority, ProjectSession, SourceUnitId, SyntaxGenerationId,
    build_typed_program, project_session_for_planned_syntax_assembly,
};
use std::sync::Arc;

const HOST: &str = r#"use Std.Probe.Bounded;
pub type Settings { pub Bounded.Limits limits, pub i32 count, pub Box<i32> boxed, }
pub type Box<T> { pub T Value, }
pub i32 LocalCount() { return 1_i32; }
pub Box<i32> RightBox() { return Box<i32> { Value: 1_i32 }; }
pub Box<u32> WrongBox() { return Box<u32> { Value: 1_u32 }; }
pub i32 Scaled(i32 factor) { return factor; }
pub T Echo<T>(T value) { return value; }
pub mod Defaults { pub i32 MakeCount() { return 2_i32; } i32 Hidden() { return 3_i32; } }
mod Sealed { pub i32 Inside() { return 4_i32; } }
"#;

const BOUNDED: &str = r#"pub type Limits { pub i64 maxWork, }
pub Limits DefaultLimits() { return Limits { maxWork: 1_i64 }; }
Limits HiddenLimits() { return Limits { maxWork: 2_i64 }; }
"#;

struct Fixture {
    _root: tempfile::TempDir,
    db: BeskidDatabase,
    project: ProjectSession,
    assembly: Arc<ProgramAssembly>,
}
impl Fixture {
    /// A host package importing a dependency package, like the legality fixtures.
    fn with_dependency() -> Self {
        let root = tempfile::tempdir().unwrap();
        let host = root.path().join("schema");
        let dependency = root.path().join("bounded");
        let path = host.join("src/Probe/Schema.bd");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, HOST).unwrap();
        let bounded = dependency.join("src/Probe/Bounded.bd");
        std::fs::create_dir_all(bounded.parent().unwrap()).unwrap();
        std::fs::write(&bounded, BOUNDED).unwrap();
        let plan = CompilePlan {
            project_root: host.clone(),
            manifest_path: host.join("schema.bproj"),
            project_name: "schema".into(),
            source_root: host.join("src"),
            target: Target { name: "Schema".into(), kind: TargetKind::Lib, entry: Some("Probe/Schema.bd".into()) },
            dependency_projects: vec![ResolvedDependencyProject {
                dependency_name: "bounded".into(),
                manifest_path: dependency.join("bounded.bproj"),
                project_root: dependency.clone(),
                project_name: "bounded".into(),
                source_root: dependency.join("src"),
            }],
            unresolved_dependencies: vec![],
            has_std_dependency: true,
        };
        Self::assemble(root, plan, &path, false)
    }
    /// A prepared single package: the host gate requires prepare-owned package identity.
    fn verified(source: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let source_root = root.path().join("Src");
        std::fs::create_dir(&source_root).unwrap();
        let path = source_root.join("Main.bd");
        std::fs::write(&path, source).unwrap();
        let plan = CompilePlan {
            project_root: root.path().to_owned(),
            manifest_path: root.path().join("Host.bproj"),
            project_name: "Host".into(),
            source_root,
            target: Target { name: "Main".into(), kind: TargetKind::Lib, entry: Some("Main.bd".into()) },
            dependency_projects: vec![],
            unresolved_dependencies: vec![],
            has_std_dependency: false,
        };
        std::fs::write(
            &plan.manifest_path,
            "Host { name = \"Host\" version = \"1.0.0\" root = \"Src\" }\ntarget \"Main\" { kind = \"Lib\" entry = \"Main.bd\" }\n",
        )
        .unwrap();
        Self::assemble(root, plan, &path, true)
    }
    fn assemble(root: tempfile::TempDir, plan: CompilePlan, path: &std::path::Path, verified: bool) -> Self {
        let workspace = verified.then(|| beskid_analysis::projects::prepare_project_workspace(&plan).unwrap());
        let assembly = Arc::new(
            assemble_program_with_materializer(
                &plan,
                workspace.as_ref(),
                path,
                None,
                &AssemblyOptions::default(),
                None,
                None,
            )
            .unwrap(),
        );
        let mut db = BeskidDatabase::default();
        let project =
            project_session_for_planned_syntax_assembly(&mut db, &assembly, &plan, path, "mod-function-test".into())
                .unwrap();
        build_typed_program(&mut db, project, assembly.generation, assembly.clone()).unwrap();
        Self { _root: root, db, project, assembly }
    }
    fn authority(&self) -> ModSemanticQueryAuthority<'_> {
        ModSemanticQueryAuthority::for_registered_assembly(&self.db, self.project, &self.assembly).unwrap()
    }
    fn type_key(&self, occurrence: usize) -> AstNodeKey {
        AstNodeKey {
            unit: SourceUnitId::new(&self.db, self.assembly.entry_unit().path.clone()),
            generation: self.assembly.generation,
            node: self.assembly.entry_syntax_index().ids_of_kind(NodeKind::TypeDefinition).nth(occurrence).unwrap(),
        }
    }
}

fn root(authority: &ModSemanticQueryAuthority<'_>, issuer: u64) -> ModSyntaxNodeRef {
    let ModSyntaxResponse::Node(Some(root)) = authority.syntax_query(issuer, &ModSyntaxRequest::Root).unwrap() else {
        panic!("registered root missing")
    };
    root
}

fn route(path: &[&str]) -> Vec<String> {
    path.iter().map(|segment| (*segment).to_owned()).collect()
}

#[test]
fn resolve_function_follows_canonical_routes_across_modules_and_packages() {
    let fixture = Fixture::with_dependency();
    let authority = fixture.authority();
    let issuer = 931;
    let caller = root(&authority, issuer);

    let local = authority.resolve_function(issuer, &caller, &route(&["LocalCount"])).unwrap();
    assert_eq!(local.generic_count, 0);
    assert!(local.parameters.is_empty());
    assert_eq!(local.result, ModSemanticFieldType::Scalar(PrimitiveType::I32));
    assert_eq!(local.declaration.source_unit, fixture.assembly.entry_unit().path);
    assert_eq!(local.declaration.generation, fixture.assembly.generation);
    let source = &fixture.assembly.entry_unit().source;
    assert!(source[local.declaration.span.start..local.declaration.span.end].contains("LocalCount"));

    let scaled = authority.resolve_function(issuer, &caller, &route(&["Scaled"])).unwrap();
    assert_eq!(scaled.parameters, [ModSemanticFieldType::Scalar(PrimitiveType::I32)]);

    let module = authority.resolve_function(issuer, &caller, &route(&["Defaults", "MakeCount"])).unwrap();
    assert_eq!(module.result, ModSemanticFieldType::Scalar(PrimitiveType::I32));
    // A private inline module declared in the caller's own scope is visible to that scope.
    authority.resolve_function(issuer, &caller, &route(&["Sealed", "Inside"])).unwrap();

    let package = authority.resolve_function(issuer, &caller, &route(&["Bounded", "DefaultLimits"])).unwrap();
    assert!(package.declaration.source_unit.ends_with("Probe/Bounded.bd"));
    let settings = authority.type_shape(authority.resolve_type(fixture.type_key(0)).unwrap()).unwrap();
    let ModSemanticShapeBody::Record { fields } = settings.body else { panic!("record shape required") };
    assert_eq!(fields[0].name, "limits");
    assert_eq!(package.result, fields[0].ty, "function result and shape field share one issued handle");
    assert!(matches!(package.result, ModSemanticFieldType::Nominal(_)));
}

#[test]
fn resolve_function_result_handles_distinguish_generic_applications() {
    let fixture = Fixture::with_dependency();
    let authority = fixture.authority();
    let issuer = 932;
    let caller = root(&authority, issuer);
    let settings = authority.type_shape(authority.resolve_type(fixture.type_key(0)).unwrap()).unwrap();
    let ModSemanticShapeBody::Record { fields } = settings.body else { panic!("record shape required") };
    assert_eq!(fields[2].name, "boxed");
    let right = authority.resolve_function(issuer, &caller, &route(&["RightBox"])).unwrap();
    let wrong = authority.resolve_function(issuer, &caller, &route(&["WrongBox"])).unwrap();
    assert_eq!(right.result, fields[2].ty, "Box<i32> result equals the Box<i32> field handle");
    assert_ne!(wrong.result, fields[2].ty, "Box<u32> result is a distinct application");
}

#[test]
fn resolve_function_denies_private_unknown_and_unspecialized_routes() {
    let fixture = Fixture::with_dependency();
    let authority = fixture.authority();
    let issuer = 933;
    let caller = root(&authority, issuer);
    assert!(
        authority.resolve_function(issuer, &caller, &route(&["Bounded", "HiddenLimits"])).is_err(),
        "private function of another package is unresolvable"
    );
    assert!(
        authority.resolve_function(issuer, &caller, &route(&["Defaults", "Hidden"])).is_err(),
        "private function of an inline module outside the caller scope is unresolvable"
    );
    assert!(authority.resolve_function(issuer, &caller, &route(&["Missing"])).is_err());
    assert!(authority.resolve_function(issuer, &caller, &route(&["Defaults", "Missing"])).is_err());
    assert!(
        authority.resolve_function(issuer, &caller, &route(&["Echo"])).is_err(),
        "a signature naming its own unspecialized generic has no concrete identity"
    );
    assert!(authority.resolve_function(issuer, &caller, &[]).is_err());
}

#[test]
fn resolve_function_rejects_foreign_stale_and_closed_callers() {
    let fixture = Fixture::with_dependency();
    let authority = fixture.authority();
    let issuer = 934;
    let caller = root(&authority, issuer);
    let path = route(&["LocalCount"]);
    assert!(authority.resolve_function(issuer + 1, &caller, &path).is_err(), "foreign invocation issuer");
    let mut stale = caller.clone();
    stale.generation = SyntaxGenerationId(stale.generation.0 + 1);
    assert!(authority.resolve_function(issuer, &stale, &path).is_err(), "stale generation");
    let mut invented = caller.clone();
    invented.node = beskid_queries::AstNodeId(1);
    assert!(authority.resolve_function(issuer, &invented, &path).is_err(), "unissued node");
    let other = fixture.authority();
    assert!(other.resolve_function(issuer, &caller, &path).is_err(), "another issuer's reference");
    authority.take_applied_syntax(issuer).unwrap();
    assert!(authority.resolve_function(issuer, &caller, &path).is_err(), "closed invocation");
}

#[test]
fn check_serializable_exposes_the_host_gate_for_opaque_resources() {
    let fixture = Fixture::verified(
        "pub type Clean { pub i32 count, pub string label, }\npub type Raw { pub pointer address, }\npub type Holder { pub Raw raw, }\n",
    );
    let authority = fixture.authority();
    let clean = authority.resolve_type(fixture.type_key(0)).unwrap();
    authority.check_serializable(clean).unwrap();
    assert!(authority.check_serializable(authority.resolve_type(fixture.type_key(1)).unwrap()).is_err());
    assert!(
        authority.check_serializable(authority.resolve_type(fixture.type_key(2)).unwrap()).is_err(),
        "a nested opaque resource rejects its owner"
    );
    let other = fixture.authority();
    assert!(other.check_serializable(clean).is_err(), "handles are checked against their issuer");
    assert!(authority.check_serializable(beskid_analysis::mod_host::ModSemanticHandle::from_token(u64::MAX)).is_err());
}

#[test]
fn check_serializable_requires_prepared_package_identity() {
    let fixture = Fixture::with_dependency();
    let authority = fixture.authority();
    let settings = authority.resolve_type(fixture.type_key(0)).unwrap();
    assert!(authority.check_serializable(settings).is_err(), "synthetic sources lack serialization identity");
}
