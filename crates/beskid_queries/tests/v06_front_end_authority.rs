//! The query layer is the only executable front end (`docs/design/2026-10-12-query-front-end.md`).
//!
//! The prepare spine assembles an `ExecutableFrontEnd` whenever it completes, projects the
//! rewritten entry into the syntax assembly codegen registers, keeps the editor recovery policy
//! through every projection, and resolves bare sibling-method calls through `call_lowering`, the
//! fact ISLE reads, instead of a second resolver.

use std::sync::Arc;

use beskid_analysis::projects::{
    AssemblyDiscovery, AssemblyRecoveryPolicy, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry,
    SourceUnit,
};
use beskid_analysis::services::{
    FrontEndOptions, PrepareOptions, parse_program_with_source_name, resolved_input_from_plan,
    synthetic_compile_plan_for_source,
};
use beskid_analysis::syntax::{Expression, SyntaxGenerationId};
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{
    AstNodeKey, BeskidDatabase, CallLowering, IndexedNodeKind, ProjectSession, SourceUnitId, build_typed_program,
    call_lowering, node_kind,
};

#[test]
fn the_prepare_spine_always_carries_the_executable_front_end() {
    let directory = tempfile::tempdir().expect("project");
    let entry_path = directory.path().join("Main.bd");
    let source = "i32 Main() { return 0; }";
    std::fs::write(&entry_path, source).expect("entry source");
    let plan = synthetic_compile_plan_for_source(&entry_path);
    let resolved = resolved_input_from_plan(entry_path.clone(), source.to_string(), plan, None, None);
    let prepared = beskid_queries::prepare_compilation(
        &resolved,
        PrepareOptions {
            front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
            ..PrepareOptions::without_mods()
        },
        None,
    )
    .expect("prepare");

    let syntax = prepared.syntax_assembly();
    assert_eq!(syntax.entry_unit().program, prepared.program, "the syntax assembly projects the prepared entry program");
    assert_eq!(syntax.entry_unit().program, prepared.executable().syntax_assembly().entry_unit().program);
    assert_eq!(prepared.executable().assembly.generation, prepared.assembly.generation);
    assert_eq!(prepared.executable().binding_plan, prepared.binding_plan);
    let executable = prepared.into_executable().expect("the executable front end is owned by the caller");
    assert_eq!(executable.program, executable.syntax_assembly().entry_unit().program);
    beskid_analysis::services::invalidate_entry_sessions_for_project(directory.path());
}

#[test]
fn a_rewritten_entry_program_is_what_the_syntax_assembly_projects() {
    let directory = tempfile::tempdir().expect("project");
    let entry_path = directory.path().join("Main.bd");
    let source = "i32 Main() { return 0; }";
    std::fs::write(&entry_path, source).expect("entry source");
    let plan = synthetic_compile_plan_for_source(&entry_path);
    let resolved = resolved_input_from_plan(entry_path.clone(), source.to_string(), plan, None, None);
    let mut front = beskid_queries::compile_mod_free_front_end(&resolved, FrontEndOptions::default(), None)
        .expect("front end");
    let rewritten =
        parse_program_with_source_name("Main.bd", "i32 Rewritten() { return 0; }").expect("rewritten entry program");
    front.program = rewritten.clone();

    let syntax_assembly = front.syntax_assembly();

    assert_eq!(syntax_assembly.entry_unit().program, rewritten);
    assert_eq!(syntax_assembly.roots, front.assembly.roots);
    assert_eq!(syntax_assembly.entry_index, front.assembly.entry_index);
    assert_eq!(syntax_assembly.discovery, front.assembly.discovery);
    assert!(Arc::ptr_eq(&syntax_assembly.module_index, &front.assembly.module_index));
    assert_eq!(syntax_assembly.has_core_dependency, front.assembly.has_core_dependency);
    beskid_analysis::services::invalidate_entry_sessions_for_project(directory.path());
}

#[test]
fn cached_editor_assembly_cannot_enter_strict_prepare_and_projections_preserve_policy() {
    let directory = tempfile::tempdir().expect("isolated policy project");
    let entry_path = directory.path().join("Main.bd");
    let source = "i32 Main() { return 0; }";
    std::fs::write(&entry_path, source).unwrap();
    let plan = synthetic_compile_plan_for_source(&entry_path);
    let mut assembly_options =
        beskid_analysis::projects::assembly_options_for_prepare(&plan, FrontEndOptions::default().assembly_discovery);
    assembly_options.recovery_policy = AssemblyRecoveryPolicy::EditorRetainRecovered;
    let cached = beskid_analysis::projects::assemble_program_with_materializer(
        &plan,
        None,
        &entry_path,
        Some(source),
        &assembly_options,
        None,
        None,
    )
    .expect("actual editor-policy assembly");
    let resolved = resolved_input_from_plan(entry_path.clone(), source.to_owned(), plan, None, Some(cached));
    let error = match beskid_queries::prepare_compilation(&resolved, PrepareOptions::without_mods(), None) {
        Ok(_) => panic!("strict preparation must not consume cached editor authority"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("recovery policy"), "must diagnose authority mismatch: {error}");

    let prepared = beskid_queries::prepare_compilation(
        &resolved,
        PrepareOptions {
            front_end: FrontEndOptions {
                assembly_recovery: AssemblyRecoveryPolicy::EditorRetainRecovered,
                ..Default::default()
            },
            ..PrepareOptions::without_mods()
        },
        None,
    )
    .expect("matching editor-policy prepare");
    assert_eq!(prepared.assembly.recovery_policy, AssemblyRecoveryPolicy::EditorRetainRecovered);
    assert_eq!(prepared.syntax_assembly().recovery_policy, AssemblyRecoveryPolicy::EditorRetainRecovered);
    assert_eq!(
        prepared.executable().syntax_assembly().recovery_policy,
        AssemblyRecoveryPolicy::EditorRetainRecovered,
        "the executable projection retains recovery authority too"
    );
    beskid_analysis::services::invalidate_entry_sessions_for_project(directory.path());
}

/// Register `source` as one single-unit program and return its database, entry unit and the
/// syntax generation.
fn registered(source: &str) -> (tempfile::TempDir, BeskidDatabase, Arc<ProgramAssembly>, SourceUnitId) {
    let directory = tempfile::tempdir().expect("project");
    let path = directory.path().join("Main.bd");
    std::fs::write(&path, source).expect("source");
    let program = parse_program_with_source_name(path.to_str().unwrap(), source).expect("parse source");
    let mut db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, path.clone());
    let project = ProjectSession::new(&db, directory.path().to_path_buf(), path.clone(), "App".into(), "lock".into());
    let generation = SyntaxGenerationId(31);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: directory.path().to_path_buf() },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: "Main".into(),
            origin_path: path.clone(),
            path,
            source: source.into(),
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    build_typed_program(&mut db, project, generation, Arc::clone(&assembly)).expect("typed syntax program");
    (directory, db, assembly, unit)
}

/// The bare single-segment calls (not `this`) that `call_lowering` resolves directly to a method
/// declaration, i.e. sibling-method dispatch on the implicit receiver.
fn method_dispatch_count(source: &str) -> usize {
    let (_directory, db, assembly, unit) = registered(source);
    let generation = assembly.generation;
    let index = assembly.entry_syntax_index();
    let program = &assembly.entry_unit().program;
    index
        .ids_of_kind(NodeKind::CallExpression)
        .filter(|node| {
            let Some(call) =
                index.node_at(program, *node).and_then(|node| node.of::<beskid_analysis::syntax::CallExpression>())
            else {
                return false;
            };
            let Expression::Path(path) = &call.callee.node else { return false };
            let [segment] = path.node.path.node.segments.as_slice() else { return false };
            if segment.node.name.node.name == "this" {
                return false;
            }
            let key = AstNodeKey { unit, generation, node: *node };
            match call_lowering(&db, key) {
                Ok(Some(CallLowering::Direct(declaration))) => {
                    matches!(node_kind(&db, declaration), Ok(Some(IndexedNodeKind::MethodDefinition)))
                }
                _ => false,
            }
        })
        .count()
}

#[test]
fn bare_sibling_method_beats_same_named_free_function() {
    // The free `Add` takes three arguments; binding the bare call to it would be an arity error.
    let source = r#"
pub type Counter {
    i64 value,

    pub i64 Add(i64 amount) { return value + amount; }

    pub i64 Twice(i64 amount) { return Add(Add(amount)); }
}

pub i64 Add(Counter counter, i64 amount, i64 extra) { return counter.value + amount + extra; }
"#;
    assert_eq!(method_dispatch_count(source), 2, "both bare `Add` calls dispatch on the implicit receiver");
}

#[test]
fn free_function_stays_callable_by_module_path_and_outside_methods() {
    let source = r#"
mod Tools {
    pub i64 Add(Counter counter, i64 amount, i64 extra) { return counter.value + amount + extra; }
}

pub type Counter {
    i64 value,

    pub i64 Add(i64 amount) { return value + amount; }

    pub i64 Both(i64 amount) { return Tools.Add(this, Add(amount), 1); }
}

pub i64 Add(Counter counter, i64 amount, i64 extra) { return counter.value + amount + extra; }

pub i64 UseFree() {
    Counter counter = Counter { value: 1 };
    return Add(counter, 2, 3);
}
"#;
    assert_eq!(method_dispatch_count(source), 1, "only the bare call inside the method is a sibling dispatch");
}

#[test]
fn bare_sibling_method_in_generic_type_dispatches_on_receiver() {
    let source = r#"
pub type Holder<T> {
    T item,

    pub bool Has() { return true; }

    pub bool Check() { return Has(); }
}
"#;
    assert_eq!(method_dispatch_count(source), 1);
}

#[test]
fn bare_receiver_field_reads_are_not_method_dispatches() {
    let source = r#"
pub type Port {
    u8 high,
    u8 low,

    pub i64 Value() { return i64(high) * 256_i64 + i64(low); }
}
"#;
    assert_eq!(method_dispatch_count(source), 0);
}
