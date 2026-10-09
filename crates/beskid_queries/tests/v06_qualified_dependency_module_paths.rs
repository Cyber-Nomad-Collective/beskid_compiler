//! Fully qualified calls resolve through the package-native module namespace of the resolved
//! dependency closure without a `use`, as in Rust and C#. Ambiguity fails closed with a
//! diagnostic, and private items stay inaccessible.
use beskid_analysis::projects::{
    AssemblyDiscovery, AssemblyOptions, CompilePlan, ProgramAssembly, ResolvedDependencyProject, Target, TargetKind,
    assemble_program_with_materializer,
};
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{
    AstNodeKey, BeskidDatabase, CallLowering, SourceUnitId, UnresolvedCallKind, build_typed_program, call_lowering,
    check_items, project_session_for_planned_syntax_assembly, unresolved_call_target,
};
use std::path::Path;
use std::sync::Arc;

/// `Base/Output/Output.bd` of the transitive `foundation` package: logical module `Base.Output`.
const FOUNDATION_OUTPUT: &str = "pub i64 Answer() { return 42; }\ni64 Hidden() { return 1; }\n";
const FOUNDATION_FILES: &[(&str, &str)] = &[("Base/Output/Output.bd", FOUNDATION_OUTPUT)];
const AGGREGATE_FILES: &[(&str, &str)] = &[("Aggregate/Aggregate.bd", "pub i64 Version() { return 1; }\n")];
const FORK_FILES: &[(&str, &str)] = &[("Base/Output/Output.bd", "pub i64 Answer() { return 7; }\n")];
const BASE_FILES: &[(&str, &str)] = &[("Base/Base.bd", "pub i64 Answer() { return 42; }\n")];

struct Fixture {
    _temporary: tempfile::TempDir,
    db: BeskidDatabase,
    assembly: Arc<ProgramAssembly>,
}

impl Fixture {
    /// The host `app` depends directly on `aggregate`, which depends on `foundation`. The
    /// resolved dependency closure lists both packages; the host source has no `use`.
    fn new(host: &str, packages: &[(&str, &[(&str, &str)])], discovery: AssemblyDiscovery) -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let app = temporary.path().join("app");
        let entry = app.join("src/App/Main.bd");
        write(&entry, host);
        let dependency_projects = packages
            .iter()
            .map(|(name, files)| {
                let root = temporary.path().join(name);
                for (relative, source) in *files {
                    write(&root.join("src").join(relative), source);
                }
                ResolvedDependencyProject {
                    dependency_name: (*name).into(),
                    manifest_path: root.join(format!("{name}.bproj")),
                    project_root: root.clone(),
                    project_name: (*name).into(),
                    source_root: root.join("src"),
                }
            })
            .collect();
        let plan = CompilePlan {
            project_root: app.clone(),
            manifest_path: app.join("app.bproj"),
            project_name: "app".into(),
            source_root: app.join("src"),
            target: Target { name: "AppCheck".into(), kind: TargetKind::Lib, entry: Some("App/Main.bd".into()) },
            dependency_projects,
            unresolved_dependencies: vec![],
            has_core_dependency: false,
        };
        let options = AssemblyOptions { discovery, ..AssemblyOptions::default() };
        let assembly =
            Arc::new(assemble_program_with_materializer(&plan, None, &entry, None, &options, None, None).unwrap());
        let mut db = BeskidDatabase::default();
        let project =
            project_session_for_planned_syntax_assembly(&mut db, &assembly, &plan, &entry, "qualified-deps".into())
                .unwrap();
        build_typed_program(&mut db, project, assembly.generation, assembly.clone()).unwrap();
        Self { _temporary: temporary, db, assembly }
    }

    fn entry_keys(&self, kind: NodeKind) -> Vec<AstNodeKey> {
        let unit = SourceUnitId::new(&self.db, self.assembly.entry_unit().path.clone());
        self.assembly
            .entry_syntax_index()
            .ids_of_kind(kind)
            .map(|node| AstNodeKey { unit, generation: self.assembly.generation, node })
            .collect()
    }
}

fn write(path: &Path, source: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
}

const AGGREGATE_AND_FOUNDATION: &[(&str, &[(&str, &str)])] =
    &[("aggregate", AGGREGATE_FILES), ("foundation", FOUNDATION_FILES)];

#[test]
fn qualified_call_into_transitive_dependency_module_resolves_without_use() {
    let fixture = Fixture::new(
        "pub i64 Main() { return Base.Output.Answer(); }\n",
        AGGREGATE_AND_FOUNDATION,
        AssemblyDiscovery::ImportClosure,
    );
    let [call] = fixture.entry_keys(NodeKind::CallExpression)[..] else { panic!("one call") };
    let Ok(Some(CallLowering::Direct(declaration))) = call_lowering(&fixture.db, call) else {
        panic!("`Base.Output.Answer` must lower Direct: {:?}", call_lowering(&fixture.db, call));
    };
    assert!(
        declaration.unit.path(&fixture.db).ends_with("foundation/src/Base/Output/Output.bd"),
        "the call targets the transitive dependency declaration"
    );
    let items = fixture.entry_keys(NodeKind::FunctionDefinition);
    assert_eq!(unresolved_call_target(&fixture.db, items[0]), Ok(None));
    assert!(check_items(&fixture.db, &items).is_ok(), "production legality accepts the fully qualified call");
}

#[test]
fn qualified_call_to_private_dependency_item_stays_inaccessible() {
    let fixture = Fixture::new(
        "pub i64 Main() { return Base.Output.Hidden(); }\n",
        AGGREGATE_AND_FOUNDATION,
        AssemblyDiscovery::ImportClosure,
    );
    let [call] = fixture.entry_keys(NodeKind::CallExpression)[..] else { panic!("one call") };
    assert!(call_lowering(&fixture.db, call).is_err(), "a private item is never a call target");
    let items = fixture.entry_keys(NodeKind::FunctionDefinition);
    let finding = unresolved_call_target(&fixture.db, items[0]).unwrap().expect("private access is reported");
    assert_eq!(
        finding.kind,
        UnresolvedCallKind::PrivateItemInModule { module_path: Arc::from("Base.Output"), name: Arc::from("Hidden") }
    );
    assert_eq!(finding.call, call);
    assert!(check_items(&fixture.db, &items).is_err(), "production legality rejects the private access");
}

#[test]
fn qualified_module_path_declared_by_two_packages_fails_closed() {
    let fixture = Fixture::new(
        "pub i64 Main() { return Base.Output.Answer(); }\n",
        &[("foundation", FOUNDATION_FILES), ("fork", FORK_FILES)],
        AssemblyDiscovery::WorkspaceScan,
    );
    let [call] = fixture.entry_keys(NodeKind::CallExpression)[..] else { panic!("one call") };
    assert!(call_lowering(&fixture.db, call).is_err(), "an ambiguous module path selects no declaration");
    let items = fixture.entry_keys(NodeKind::FunctionDefinition);
    let finding = unresolved_call_target(&fixture.db, items[0]).unwrap().expect("ambiguity is reported");
    assert_eq!(finding.kind, UnresolvedCallKind::AmbiguousModulePath { path: Arc::from("Base.Output"), candidates: 2 });
    let findings = check_items(&fixture.db, &items).expect_err("production legality rejects the ambiguity");
    assert!(findings.iter().any(|finding| finding.kind.code() == "E1104"), "{findings:?}");
}

#[test]
fn lexical_inline_module_shadows_dependency_module_of_the_same_root() {
    let fixture = Fixture::new(
        "mod Base { pub i64 Answer() { return 7; } }\npub i64 Main() { return Base.Answer(); }\n",
        &[("foundation", BASE_FILES)],
        AssemblyDiscovery::WorkspaceScan,
    );
    let [call] = fixture.entry_keys(NodeKind::CallExpression)[..] else { panic!("one call") };
    let [local, _main] = fixture.entry_keys(NodeKind::FunctionDefinition)[..] else { panic!("two functions") };
    assert_eq!(
        call_lowering(&fixture.db, call),
        Ok(Some(CallLowering::Direct(local))),
        "the lexical inline module owns the root; the absolute namespace is not consulted"
    );
}
