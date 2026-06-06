//! Inline methods inside `type { }` bodies: parse, dispatch, and duplicate-member rules.

use beskid_analysis::services::parse_program_with_source_name;
use beskid_analysis::syntax::Node;

#[test]
fn type_body_parses_inline_method() {
    let program = parse_program_with_source_name(
        "test.bd",
        r#"
type Counter {
    i32 value,

    pub unit Increment() {
        value += 1;
    }
}
"#,
    )
    .expect("parse type with inline method");
    let Node::TypeDefinition(type_def) = &program.node.items[0].node else {
        panic!("expected type definition");
    };
    assert_eq!(type_def.node.fields.len(), 1);
    assert_eq!(type_def.node.methods.len(), 1);
    assert_eq!(type_def.node.methods[0].node.name.node.name, "Increment");
}

#[test]
fn generic_type_body_parses_inline_method() {
    let program = parse_program_with_source_name(
        "test.bd",
        r#"
type Container<T> {
    T item,

    pub T Get() {
        return item;
    }
}
"#,
    )
    .expect("parse generic type with inline method");
    let Node::TypeDefinition(type_def) = &program.node.items[0].node else {
        panic!("expected type definition");
    };
    assert_eq!(type_def.node.generics.len(), 1);
    assert_eq!(type_def.node.methods.len(), 1);
    assert_eq!(type_def.node.methods[0].node.name.node.name, "Get");
}

#[test]
fn duplicate_field_and_method_name_is_parseable() {
    // Parser accepts the surface; resolution reports duplicate member.
    let program = parse_program_with_source_name(
        "test.bd",
        r#"
type Bad {
    i32 value,

    pub unit value() {
        return;
    }
}
"#,
    )
    .expect("parse type with duplicate field/method name");
    let Node::TypeDefinition(type_def) = &program.node.items[0].node else {
        panic!("expected type definition");
    };
    assert_eq!(type_def.node.fields.len(), 1);
    assert_eq!(type_def.node.methods.len(), 1);
}

#[test]
fn duplicate_field_and_method_name_errors_on_resolve() {
    use beskid_analysis::hir::{AstProgram, HirProgram, lower_program, normalize_program};
    use beskid_analysis::resolve::{ResolveError, Resolver};
    use beskid_analysis::syntax::Spanned;

    let program = parse_program_with_source_name(
        "test.bd",
        r#"
type Bad {
    i32 value,

    pub unit value() {
        return;
    }
}
"#,
    )
    .expect("parse");
    let ast: Spanned<AstProgram> = program.into();
    let mut hir: Spanned<HirProgram> = lower_program(&ast);
    normalize_program(&mut hir).expect("normalize");
    let errors = Resolver::new()
        .resolve_program(&hir)
        .expect_err("expected duplicate member error");
    assert!(matches!(
        errors.first(),
        Some(ResolveError::DuplicateItem { .. })
    ));
}

#[test]
fn inline_method_call_typechecks() {
    use std::path::PathBuf;

    use beskid_analysis::services::{
        FrontEndOptions, ResolvedInput, compile_front_end_from_resolved_input, resolve_input,
    };

    use crate::projects::with_cwd_at_workspace_root;

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("compiler workspace root")
        .to_path_buf();
    let entry = root
        .join("corelib/beskid_corelib/tests/corelib_tests/src/concurrency/MutexTryLockTests.bd");
    if !entry.is_file() {
        return;
    }
    let project_root = entry
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let source = std::fs::read_to_string(&entry).expect("read MutexTryLockTests.bd");

    let resolved = with_cwd_at_workspace_root(&root, || {
        resolve_input(Some(&entry), Some(&project_root), None, None, false, false).expect("resolve")
    });

    let plan = resolved.compile_plan.expect("compile plan");
    let resolved_input = ResolvedInput {
        source_path: entry,
        source,
        compile_plan: Some(plan),
        prepared_workspace: resolved.prepared_workspace,
        workspace_summary: resolved.workspace_summary,
        assembly: None,
    };

    compile_front_end_from_resolved_input(
        &resolved_input,
        FrontEndOptions {
            with_semantic_diagnostics: false,
            ..Default::default()
        },
        None,
    )
    .expect("inline Mutex.TryLock front-end must type-check");
}
