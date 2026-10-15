//! CLI legality authority must preserve lexical nominal identity across imported modules.
use beskid_analysis::projects::{
    AssemblyOptions, CompilePlan, ProgramAssembly, ResolvedDependencyProject, Target, TargetKind,
    assemble_program_with_materializer,
};
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{
    AstNodeKey, BeskidDatabase, SourceUnitId, aggregate_field_access, build_typed_program, check_items,
    project_session_for_planned_syntax_assembly, unresolved_type_reference,
};
use std::sync::Arc;

fn fixture(source: &str) -> (tempfile::TempDir, BeskidDatabase, Arc<ProgramAssembly>, Vec<AstNodeKey>) {
    let temporary = tempfile::tempdir().unwrap();
    let host = temporary.path().join("schema");
    let dependency = temporary.path().join("bounded");
    let path = host.join("src/Probe/Schema.bd");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, source).unwrap();
    let bounded = dependency.join("src/Probe/Bounded.bd");
    std::fs::create_dir_all(bounded.parent().unwrap()).unwrap();
    std::fs::write(&bounded, "pub type PatternLimits { pub i64 maxWork, }").unwrap();
    let other = dependency.join("src/Probe/Other.bd");
    std::fs::write(&other, "pub type PatternLimits { pub bool enabled, }").unwrap();
    let plan = CompilePlan {
        project_root: host.clone(),
        manifest_path: host.join("schema.bproj"),
        project_name: "schema".into(),
        source_root: host.join("src"),
        target: Target { name: "SchemaCheck".into(), kind: TargetKind::Lib, entry: Some("Probe/Schema.bd".into()) },
        dependency_projects: vec![ResolvedDependencyProject {
            dependency_name: "bounded".into(),
            manifest_path: dependency.join("bounded.bproj"),
            project_root: dependency.clone(),
            project_name: "bounded".into(),
            source_root: dependency.join("src"),
        }],
        unresolved_dependencies: vec![],
        has_core_dependency: false,
    };
    let assembly = Arc::new(
        assemble_program_with_materializer(&plan, None, &path, None, &AssemblyOptions::default(), None, None).unwrap(),
    );
    let mut db = BeskidDatabase::default();
    let project =
        project_session_for_planned_syntax_assembly(&mut db, &assembly, &plan, &path, "shadow-legality".into())
            .unwrap();
    build_typed_program(&mut db, project, assembly.generation, assembly.clone()).unwrap();
    let unit = SourceUnitId::new(&db, assembly.entry_unit().path.clone());
    let items = assembly
        .entry_syntax_index()
        .ids_of_kind(NodeKind::FunctionDefinition)
        .map(|node| AstNodeKey { unit, generation: assembly.generation, node })
        .collect();
    (temporary, db, assembly, items)
}

#[test]
fn local_type_shadows_imported_same_leaf_at_parameter_legality() {
    let (_temporary, db, _assembly, items) = fixture(
        "use Probe.Bounded;\npub type PatternLimits { pub i64 maxDepth, }\npub i64 ValidateWithPatternLimits(PatternLimits policy) { return policy.maxDepth; }\n",
    );
    assert_eq!(
        unresolved_type_reference(&db, items[0]),
        Ok(None),
        "local declaration must own the parameter's nominal identity"
    );
    assert!(check_items(&db, &items).is_ok(), "production CLI legality gate must accept lexical shadowing");
}

#[test]
fn qualified_import_and_local_same_leaf_keep_distinct_field_declarations() {
    let (_temporary, db, assembly, items) = fixture(
        "use Probe.Bounded;\npub type PatternLimits { pub i64 maxDepth, }\npub i64 Local(PatternLimits policy) { return policy.maxDepth; }\npub i64 Imported(Bounded.PatternLimits policy) { return policy.maxWork; }\n",
    );
    assert!(check_items(&db, &items).is_ok(), "both scoped signatures are legal");
    let paths = assembly.entry_syntax_index().ids_of_kind(NodeKind::PathExpression).collect::<Vec<_>>();
    let unit = items[0].unit;
    let mut owners = Vec::new();
    for node in paths {
        if let Ok(Some(access)) =
            aggregate_field_access(&db, AstNodeKey { unit, generation: assembly.generation, node })
        {
            owners.push(access.declaration.unit.path(&db).clone());
        }
    }
    assert_eq!(owners.len(), 2, "both actual parameter projections resolve");
    assert_eq!(owners[0], assembly.entry_unit().path, "local signature retains Schema declaration");
    assert!(owners[1].ends_with("Probe/Bounded.bd"), "qualified signature retains imported declaration");
    assert_ne!(owners[0], owners[1], "same leaf never merges nominal owners");
}

#[test]
fn conflicting_imports_without_local_declaration_remain_rejected() {
    let (_temporary, db, _assembly, items) =
        fixture("use Probe.Bounded;\nuse Probe.Other;\npub unit Invalid(PatternLimits policy) {}\n");
    let finding =
        unresolved_type_reference(&db, items[0]).unwrap().expect("unqualified import ambiguity must stay unavailable");
    assert_eq!(finding.name.as_ref(), "PatternLimits");
    assert!(check_items(&db, &items).is_err(), "production legality rejects ambiguous imports");
}
