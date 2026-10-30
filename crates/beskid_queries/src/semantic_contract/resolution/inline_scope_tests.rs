//! Inline modules are lexical scopes: a constant, and a `use`, bind in the scope that declares
//! it and in the inline modules nested in it; a qualifier that names both an inline module and
//! an import, where both reach the member, names nothing.

use super::*;
use crate::{BeskidDatabase, build_typed_program, project_session_for_planned_syntax_assembly};
use beskid_analysis::projects::{
    AssemblyOptions, CompilePlan, ProgramAssembly, Target, TargetKind, assemble_program_with_materializer,
};
use beskid_analysis::syntax_query::NodeKind;

fn fixture(files: &[(&str, &str)]) -> (tempfile::TempDir, BeskidDatabase, Arc<ProgramAssembly>) {
    let root = tempfile::tempdir().unwrap();
    let src = root.path().join("Src");
    for (relative, source) in files {
        let path = src.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, source).unwrap();
    }
    let entry = src.join("Main.bd");
    let plan = CompilePlan {
        project_root: root.path().into(),
        manifest_path: root.path().join("Host.bproj"),
        project_name: "Host".into(),
        source_root: src,
        target: Target { name: "Main".into(), kind: TargetKind::Lib, entry: Some("Main.bd".into()) },
        dependency_projects: vec![],
        unresolved_dependencies: vec![],
        has_core_dependency: false,
        corelib_root: None,
    };
    let assembly = Arc::new(
        assemble_program_with_materializer(&plan, None, &entry, None, &AssemblyOptions::default(), None, None).unwrap(),
    );
    let mut db = BeskidDatabase::default();
    let project =
        project_session_for_planned_syntax_assembly(&mut db, &assembly, &plan, &entry, "inline-scopes".into()).unwrap();
    build_typed_program(&mut db, project, assembly.generation, assembly.clone()).unwrap();
    (root, db, assembly)
}

/// Every path expression of the entry unit whose single or last segment is `name`, in source
/// order, with its key.
fn paths_named(
    db: &BeskidDatabase,
    assembly: &ProgramAssembly,
    name: &str,
) -> Vec<(AstNodeKey, beskid_analysis::syntax::Path)> {
    let unit = SourceUnitId::new(db, assembly.entry_unit().path.clone());
    let syntax = db.syntax_unit(unit).expect("entry unit");
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let mut found = index
        .ids_of_kind(NodeKind::PathExpression)
        .filter_map(|node| {
            let path = index.node_at(program, node)?.of::<beskid_analysis::syntax::PathExpression>()?;
            let last = path.path.node.segments.last()?;
            (last.node.name.node.name == name)
                .then(|| (AstNodeKey { unit, generation: assembly.generation, node }, path.path.node.clone()))
        })
        .collect::<Vec<_>>();
    found.sort_by_key(|(key, _)| key.node.0);
    found
}

#[test]
fn constants_resolve_through_the_inline_module_scope_chain() {
    let (_directory, db, assembly) = fixture(&[(
        "Main.bd",
        "const Limit = 7;\n\
         mod T { const Limit = 9; pub i64 Inner() { return Limit; } }\n\
         mod U { mod V { pub i64 Deep() { return Limit; } } }\n\
         pub i64 Outer() { return Limit; }",
    )]);
    let values = paths_named(&db, &assembly, "Limit")
        .into_iter()
        .map(|(key, _)| constant_integer(&db, key).expect("constant fact"))
        .collect::<Vec<_>>();
    assert_eq!(values, vec![Some(9), Some(7), Some(7)]);
}

#[test]
fn a_use_inside_an_inline_module_is_visible_in_that_module_only() {
    let (_directory, db, assembly) = fixture(&[
        ("Lib/Shapes.bd", "pub type Square { i64 side; } pub i64 Make() { return 1_i64; }"),
        (
            "Main.bd",
            "mod A { use Lib.Shapes; pub i64 Inside() { return Shapes.Make(); } }\n\
             pub i64 Outside() { return Shapes.Make(); }",
        ),
    ]);
    let unit = SourceUnitId::new(&db, assembly.entry_unit().path.clone());
    let syntax = db.syntax_unit(unit).expect("entry unit");
    let (program, index) = (syntax.expanded_program(&db), syntax.syntax_index(&db));
    let resolved = paths_named(&db, &assembly, "Make")
        .into_iter()
        .map(|(key, path)| resolve_item_declaration(&db, program, index, key, &path).is_some())
        .collect::<Vec<_>>();
    assert_eq!(resolved, vec![true, false], "inside `A` the import binds; at the file scope it does not");
}

#[test]
fn an_inline_module_and_an_import_that_both_reach_the_member_name_nothing() {
    let (_directory, db, assembly) = fixture(&[
        ("Lib/Shapes.bd", "pub i64 Make() { return 1_i64; }"),
        (
            "Main.bd",
            "use Lib.Shapes;\n\
             mod Shapes { pub i64 Make() { return 2_i64; } pub i64 Other() { return 3_i64; } }\n\
             pub i64 Both() { return Shapes.Make(); }\n\
             pub i64 One() { return Shapes.Other(); }",
        ),
    ]);
    let unit = SourceUnitId::new(&db, assembly.entry_unit().path.clone());
    let syntax = db.syntax_unit(unit).expect("entry unit");
    let (program, index) = (syntax.expanded_program(&db), syntax.syntax_index(&db));
    for (name, expected) in [("Make", false), ("Other", true)] {
        let paths = paths_named(&db, &assembly, name);
        let [(key, path)] = paths.as_slice() else { panic!("one `{name}` path: {paths:?}") };
        assert_eq!(resolve_item_declaration(&db, program, index, *key, path).is_some(), expected, "`Shapes.{name}`");
    }
}
