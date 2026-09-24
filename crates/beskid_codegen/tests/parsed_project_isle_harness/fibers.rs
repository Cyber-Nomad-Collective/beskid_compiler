//! Spawn handles, fiber joins, fiber ownership, and direct spawn trampolines.

use super::*;

#[test]
fn bound_spawn_returns_runtime_handle_without_invoking_child_in_parent() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "pub type Fiber<T> { i64 handle, } i64 Compute() { return 7_i64; } i64 Main() { let child = spawn Compute(); return 0_i64; }";
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();
    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    let main = lowered.artifact.functions.iter().find(|function| function.name.starts_with("Main#syntax_")).unwrap();
    let calls = main
        .function
        .layout
        .blocks()
        .flat_map(|block| main.function.layout.block_insts(block))
        .filter(|inst| main.function.dfg.insts[*inst].opcode() == cranelift_codegen::ir::Opcode::Call)
        .count();
    assert_eq!(
        calls,
        4,
        "register child, allocate nominal handle, register/unregister its GC root; only the scheduler invokes the child\n{}",
        main.function.display()
    );
    assert_eq!(
        main.function.sized_stack_slots.len(),
        1,
        "the only parent slot is the nominal handle GC root, never a cancellation slot"
    );
}

#[test]
fn canonical_fiber_join_lowers_the_typed_traced_result_move() {
    let application = "use Concurrency.Fiber; use Concurrency.FiberError; use Core.Results; i64 Compute() { return 42_i64; } i64 Main() { let child = spawn Compute(); Result<i64, FiberError> result = child.Join(); return match result { Result::Ok(value) => value, Result::Error(_) => 0_i64, }; }";
    let temporary = tempfile::tempdir().unwrap();
    let compiler = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap();
    let concurrency = compiler.join("corelib/packages/concurrency/src");
    let foundation = compiler.join("corelib/packages/foundation/src");
    std::fs::write(temporary.path().join("Main.bd"), application).unwrap();
    let mut sources = vec![(temporary.path().join("Main.bd"), "Main.bd".to_owned(), application.to_owned())];
    for (root, relative) in [
        (&concurrency, "Concurrency/Fiber.bd"),
        (&concurrency, "Concurrency/FiberError.bd"),
        (&foundation, "Core/Results/Results.bd"),
    ] {
        let path = root.join(relative);
        sources.push((path.clone(), relative.to_owned(), std::fs::read_to_string(path).unwrap()));
    }
    let units = sources
        .into_iter()
        .map(|(path, logical_name, source)| {
            let program = parse_program_with_source_name(path.to_str().unwrap(), &source).unwrap();
            SourceUnit { origin_path: path.clone(), path, logical_name, source, program }
        })
        .collect();
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: temporary.path().to_owned() },
            dependencies: vec![
                RootEntry { dependency_name: Some("concurrency".into()), source_root: concurrency },
                RootEntry { dependency_name: Some("foundation".into()), source_root: foundation },
            ],
        },
        Arc::new(units),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        SyntaxGenerationId(0),
    ));
    let (target, isa) = x86_64_target_and_isa();
    let mut db = beskid_queries::BeskidDatabase::default();
    let lowered = lower_syntax_assembly_entrypoint(&mut db, assembly, "Main", target, isa.as_ref())
        .expect("canonical Fiber<T> source lowers");
    assert!(lowered.artifact.extern_imports.iter().any(|import| import.symbol == "fiber_join_value"));
    assert!(lowered.artifact.extern_imports.iter().any(|import| import.symbol == "beskid_rt_v5_abi_value_clear"));
}

#[test]
fn reachable_callable_rejects_moved_fiber_parameters_without_local_spawn() {
    for body in ["child.Join(); child.Join();", "child.Detach(); child.Cancel();"] {
        let source = format!(
            "pub type Fiber<T> {{ i64 handle, pub unit Join() {{ return; }} pub unit Detach() {{ return; }} pub unit Cancel() {{ return; }} }} unit Consume(Fiber<i64> child) {{ {body} return; }} unit Main() {{ Fiber<i64> child = Fiber<i64> {{ handle: 1_i64 }}; Consume(child); return; }}"
        );
        let temporary = tempfile::tempdir().unwrap();
        let assembly = parse_production_units(temporary.path(), &[("Main.bd", "Main", &source)]);
        let (target, isa) = x86_64_target_and_isa();
        let error = with_db(|db| lower_syntax_assembly_entrypoint(db, assembly, "Main", target, isa.as_ref()))
            .err()
            .expect("moved Fiber rejected");
        assert!(error.to_string().contains("UseAfterMove"), "{body}: {error}");
    }
}

#[test]
fn reachable_callable_rejects_repeatable_fiber_capture_and_inferred_method_double_join() {
    for body in [
        "let child = factory.Start(); let run = () => child.Join(); run(); run();",
        "let child = factory.Start(); child.Join(); child.Join();",
    ] {
        let source = format!(
            "pub type Fiber<T> {{ i64 handle, pub unit Join() {{ return; }} }} pub type Factory<T> {{ T value, pub Fiber<T> Start() {{ return Fiber<T> {{ handle: 1_i64 }}; }} }} unit Consume(Factory<i64> factory) {{ {body} return; }} unit Main() {{ Factory<i64> factory = Factory<i64> {{ value: 1_i64 }}; Consume(factory); return; }}"
        );
        let temporary = tempfile::tempdir().unwrap();
        let assembly = parse_production_units(temporary.path(), &[("Main.bd", "Main", &source)]);
        let (target, isa) = x86_64_target_and_isa();
        let error = with_db(|db| lower_syntax_assembly_entrypoint(db, assembly, "Main", target, isa.as_ref()))
            .err()
            .expect("invalid inferred Fiber ownership rejected before emission");
        assert!(error.to_string().contains("UseAfterMove"), "{body}: {error}");
    }
}

#[test]
fn one_shot_spawn_capture_entry_spellings_reach_production_lowering() {
    for suffix in ["", "()"] {
        let source = format!(
            "pub type Fiber<T> {{ i64 handle, pub i64 Join() {{ return 1_i64; }} }} i64 Compute() {{ return 42_i64; }} i64 Main() {{ let child = spawn Compute(); let next = spawn (() => child.Join()){suffix}; return 0_i64; }}"
        );
        let project = tempfile::tempdir().unwrap();
        let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", &source)]);
        let (target, isa) = x86_64_target_and_isa();
        let lowered = with_db(|db| lower_syntax_assembly_entrypoint(db, assembly, "Main", target, isa.as_ref()))
            .unwrap_or_else(|error| panic!("one-shot spawn suffix {suffix:?}: {error}"));
        assert!(lowered.artifact.extern_imports.iter().any(|import| import.symbol == "fiber_spawn"));
    }
}

#[test]
fn parsed_direct_zero_argument_spawn_emits_syntax_owned_trampoline_and_fiber_dispatch() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        i64 Entry() { return 7_i64; }
        pub type Fiber<T> { i64 handle, }
        i64 Main() { let child = spawn Entry; return 0_i64; }
    ";
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    assert_eq!(lowered.artifact.functions.len(), 3, "Entry, Main, and the syntax-owned spawn trampoline");
    let main = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Main#syntax_"))
        .expect("Main artifact");
    let trampoline = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("__beskid_spawn_entry_syntax_"))
        .expect("syntax-owned spawn trampoline");
    let main_clif = main.function.display().to_string();
    let trampoline_clif = trampoline.function.display().to_string();
    assert!(main_clif.contains("fiber_spawn"), "{main_clif}");
    assert!(main_clif.contains("func_addr"), "{main_clif}");
    assert!(trampoline_clif.contains("Entry#syntax_"), "{trampoline_clif}");
    assert!(trampoline_clif.contains("return"), "{trampoline_clif}");
}
