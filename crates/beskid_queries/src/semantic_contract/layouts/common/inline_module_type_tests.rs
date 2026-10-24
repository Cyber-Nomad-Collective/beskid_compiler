//! Qualified type paths into inline modules (`A.Clock` for `mod A { pub type Clock { .. } }`):
//! slice host keys composition services by full type identity, so two same-named types in two
//! inline modules must resolve to two declarations.

use super::*;
use crate::{BeskidDatabase, build_typed_program, project_session_for_planned_syntax_assembly};
use beskid_analysis::projects::{
    AssemblyOptions, CompilePlan, ProgramAssembly, Target, TargetKind, assemble_program_with_materializer,
};

fn fixture(source: &str) -> (tempfile::TempDir, BeskidDatabase, Arc<ProgramAssembly>) {
    let root = tempfile::tempdir().unwrap();
    let src = root.path().join("Src");
    std::fs::create_dir(&src).unwrap();
    let path = src.join("Main.bd");
    std::fs::write(&path, source).unwrap();
    let plan = CompilePlan {
        project_root: root.path().into(),
        manifest_path: root.path().join("Host.bproj"),
        project_name: "Host".into(),
        source_root: src,
        target: Target { name: "Main".into(), kind: TargetKind::Lib, entry: Some("Main.bd".into()) },
        dependency_projects: vec![],
        unresolved_dependencies: vec![],
        has_core_dependency: false,
    };
    let assembly = Arc::new(
        assemble_program_with_materializer(&plan, None, &path, None, &AssemblyOptions::default(), None, None).unwrap(),
    );
    let mut db = BeskidDatabase::default();
    let project =
        project_session_for_planned_syntax_assembly(&mut db, &assembly, &plan, &path, "inline-module-types".into())
            .unwrap();
    build_typed_program(&mut db, project, assembly.generation, assembly.clone()).unwrap();
    (root, db, assembly)
}

const SOURCE: &str = "mod A { pub type Clock { i64 now; } mod Inner { pub type Clock { i64 deep; } } }\n\
    mod B { pub type Clock { i64 tick; } type Hidden { i64 secret; } }\n\
    pub i64 Read(A.Clock first, B.Clock second, A.Inner.Clock nested) { return 0_i64; }\n\
    pub i64 Leak(B.Hidden hidden) { return 0_i64; }";

/// The declaration each parameter's written type path resolves to, by parameter name.
fn parameter_types() -> (tempfile::TempDir, BeskidDatabase, Vec<(String, Option<AstNodeKey>)>) {
    let (directory, db, assembly) = fixture(SOURCE);
    let unit = SourceUnitId::new(&db, assembly.entry_unit().path.clone());
    let root = AstNodeKey { unit, generation: assembly.generation, node: beskid_analysis::syntax::AstNodeId(0) };
    let syntax = db.syntax_unit(root.unit).expect("unit");
    let program = syntax.expanded_program(&db);
    let index = syntax.syntax_index(&db);
    let resolved = index
        .ids_of_kind(beskid_analysis::syntax_query::NodeKind::Parameter)
        .filter_map(|node| {
            let parameter = index.node_at(program, node)?.of::<beskid_analysis::syntax::Parameter>()?;
            let beskid_analysis::syntax::Type::Complex(path) = &parameter.ty.node else { return None };
            let key = AstNodeKey { node, ..root };
            Some((parameter.name.node.name.clone(), resolve_type_declaration(&db, key, &path.node)))
        })
        .collect::<Vec<_>>();
    (directory, db, resolved)
}

fn identity(db: &dyn Db, resolved: &[(String, Option<AstNodeKey>)], parameter: &str) -> Option<String> {
    let (_, declaration) = resolved.iter().find(|(name, _)| name == parameter).expect("parameter");
    declaration.and_then(|declaration| stable_declaration_identity(db, declaration)).map(|identity| identity.to_string())
}

#[test]
fn a_qualified_path_resolves_a_type_in_an_inline_module() {
    let (_directory, db, resolved) = parameter_types();
    assert!(identity(&db, &resolved, "first").is_some_and(|identity| identity.ends_with("A::Clock")), "{resolved:?}");
}

#[test]
fn a_qualified_path_resolves_a_type_in_a_nested_inline_module() {
    let (_directory, db, resolved) = parameter_types();
    assert!(
        identity(&db, &resolved, "nested").is_some_and(|identity| identity.ends_with("A::Inner::Clock")),
        "{resolved:?}"
    );
}

#[test]
fn same_simple_name_in_two_inline_modules_resolves_to_two_declarations() {
    let (_directory, db, resolved) = parameter_types();
    let first = resolved.iter().find(|(name, _)| name == "first").and_then(|(_, declaration)| *declaration);
    let second = resolved.iter().find(|(name, _)| name == "second").and_then(|(_, declaration)| *declaration);
    assert!(first.is_some() && second.is_some(), "{resolved:?}");
    assert_ne!(first, second);
    assert!(identity(&db, &resolved, "second").is_some_and(|identity| identity.ends_with("B::Clock")), "{resolved:?}");
}

#[test]
fn a_private_type_of_an_inline_module_is_not_visible_outside_it() {
    let (_directory, _db, resolved) = parameter_types();
    let hidden = resolved.iter().find(|(name, _)| name == "hidden").expect("hidden parameter");
    assert_eq!(hidden.1, None);
}
