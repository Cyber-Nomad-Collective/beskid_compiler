use std::{collections::BTreeSet, sync::Arc};

use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};
use beskid_analysis::{
    projects::{AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit},
    services::parse_program_with_source_name,
};
use beskid_codegen::cranelift_host::collect_validated_extern_signatures;
use beskid_codegen::{lower_canonical_runtime_prepared_syntax, lower_syntax_assembly_entrypoint};
use beskid_queries::{
    AstNodeId, AstNodeKey, SourceUnitId, SyntaxGenerationId, child_nodes, closure_environment, node_kind, with_db,
};
use cranelift_codegen::{Context, control::ControlPlane, ir::ExternalName, isa, settings, verify_function};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::default_libcall_names;

#[test]
fn retired_public_codegen_facade_is_absent() {
    let public_services = include_str!("../src/services.rs");
    for retired_api in [
        "pub struct LoweredProgram",
        "pub fn lower_source",
        "pub fn lower_source_for_entrypoint",
        "pub fn lower_source_with_pipeline",
        "pub fn lower_resolved_input_with_pipeline",
        "pub fn lower_from_prepared_or_cache",
        "pub fn lower_resolved_entrypoint_with_pipeline",
        "pub fn lower_from_front_end",
    ] {
        assert!(
            !public_services.contains(retired_api),
            "retired public codegen facade must not expose `{retired_api}`"
        );
    }
}

#[test]
fn contract_specialization_module_admission_rejects_unproved_calls_and_hidden_members() {
    for (implementation, body) in [
        ("type Source { pub i64 Read() { return 1_i64; } }", "return reader.Read();"),
        ("type Source: Reader {}", "return reader.Read();"),
        ("type Source: Reader { i64 Read() { return 1_i64; } }", "return reader.Read();"),
        ("type Source: Reader { pub i32 Read() { return 1; } }", "return reader.Read();"),
        (
            "type Source: Reader { pub i64 Read() { return 1_i64; } pub i64 Reset() { return 2_i64; } }",
            "return reader.Reset();",
        ),
    ] {
        let source = format!(
            "contract Reader {{ i64 Read(); }} {implementation} i64 ReadOne(Reader reader) {{ {body} }} i64 Main() {{ return ReadOne(Source {{}}); }}"
        );
        let root = tempfile::tempdir().unwrap();
        let assembly = parse_production_units(root.path(), &[("Main.bd", "Main.bd", &source)]);
        let (target, isa) = x86_64_target_and_isa();
        let mut db = beskid_queries::BeskidDatabase::default();
        assert!(
            lower_syntax_assembly_entrypoint(&mut db, assembly, "Main", target, isa.as_ref()).is_err(),
            "invalid contract call must not emit: {source}"
        );
    }
}

fn parse_production_units(root: &std::path::Path, units: &[(&str, &str, &str)]) -> Arc<ProgramAssembly> {
    let mut source_units = Vec::with_capacity(units.len());
    for (relative_path, logical_name, source) in units {
        let path = root.join(relative_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("unit parent directory");
        }
        std::fs::write(&path, source).expect("write project source");
        let program = parse_program_with_source_name(path.to_str().expect("UTF-8 source path"), source)
            .expect("production source parse");
        source_units.push(SourceUnit {
            logical_name: (*logical_name).into(),
            origin_path: path.clone(),
            path,
            source: (*source).into(),
            program,
        });
    }
    Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.to_path_buf() },
            dependencies: Vec::new(),
        },
        Arc::new(source_units),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        SyntaxGenerationId(0),
    ))
}

fn x86_64_target_and_isa() -> (TargetMetadata, std::sync::Arc<dyn cranelift_codegen::isa::TargetIsa>) {
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("Linux x86_64 ABI target");
    let settings = beskid_codegen::cranelift_host::production_isa_settings_builder().expect("production ISA settings");
    let isa =
        isa::lookup_by_name("x86_64").expect("x86 ISA").finish(settings::Flags::new(settings)).expect("finish ISA");
    (target, isa)
}

fn lower_verified_entrypoint(
    assembly: Arc<ProgramAssembly>,
    target: TargetMetadata,
    isa: &dyn cranelift_codegen::isa::TargetIsa,
) -> beskid_codegen::PreparedSyntaxEntrypoint {
    let lowered = with_db(|db| lower_syntax_assembly_entrypoint(db, assembly, "Main", target, isa))
        .expect("parsed project lowers through CodegenInput and ISLE");
    assert!(
        lowered.symbol.starts_with("Main#syntax_"),
        "production path must mint a syntax-mangled entry symbol, got {}",
        lowered.symbol
    );
    for function in &lowered.artifact.functions {
        verify_function(&function.function, isa.flags())
            .unwrap_or_else(|error| panic!("stock CLIF verifier rejected {}: {error}", function.name));
    }
    lowered
}

fn assert_unsupported_closed_failure(
    assembly: Arc<ProgramAssembly>,
    target: TargetMetadata,
    isa: &dyn cranelift_codegen::isa::TargetIsa,
    expected_site_fragments: &[&str],
) {
    let result = with_db(|db| lower_syntax_assembly_entrypoint(db, assembly, "Main", target, isa));
    let error = match result {
        Ok(_) => panic!("unsupported typed operation must not fall back to legacy codegen"),
        Err(error) => error,
    };
    let rendered = error.to_string();
    assert!(rendered.contains("MissingRuleOrFact") || rendered.contains("spawn legality rejected"), "{rendered}");
    for fragment in expected_site_fragments {
        assert!(rendered.contains(fragment), "expected {fragment:?} in {rendered}");
    }
}

#[test]
fn parsed_project_reaches_verified_isle_without_a_legacy_codegen_entrypoint() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        type Pair { i32 left, i32 right }
        i32 Add(i32 left, i32 right) { return left + right; }
        i32 Main() {
            Pair pair = Pair { left: 19, right: 23 };
            if pair.left < pair.right { return Add(pair.left, pair.right); }
            return 0;
        }
    ";
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    // The production entrypoint accepts parsed ProgramAssembly data and derives typed facts.
    let lowered = lower_verified_entrypoint(assembly, target.clone(), isa.as_ref());
    assert_eq!(lowered.artifact.functions.len(), 2, "reachable direct-call closure");

    let unsupported_source = "
        i32 Main() {
            i32 outer = 1;
            let task = spawn ((i32 inner) => outer + inner);
            return outer;
        }
    ";
    let unsupported = parse_production_units(project.path(), &[("Unsupported.bd", "Main", unsupported_source)]);
    // The generation-bound entry fact rejects the parameterized spawn before emitting its body.
    assert_unsupported_closed_failure(
        unsupported,
        target,
        isa.as_ref(),
        &["Unsupported.bd", "TargetRequiresArguments"],
    );
}

#[test]
fn parsed_direct_pointer_guard_with_unit_early_return_emits_verified_clif() {
    // Keep this shape aligned with the canonical scheduler's initialization guard:
    // `mut pointer table = SchedTable(); if table != NativePointer(0) { return; }`.
    // The recursive conversion helper is never executed; it supplies the exact direct-call
    // ABI shape so this test exercises statement lowering and imports rather than a host shim.
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        pointer NativePointer(word value) { return NativePointer(value); }
        pointer SchedTable() { return NativePointer(0); }
        unit Main() {
            mut pointer table = SchedTable();
            if table != NativePointer(0) { return; }
            return;
        }
    ";
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    let main = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Main#syntax_"))
        .expect("Main artifact");
    let clif = main.function.display().to_string();
    assert!(clif.contains("call"), "direct SchedTable/NativePointer calls must lower: {clif}");
    assert!(clif.contains("brif"), "the no-else pointer guard must branch in CLIF: {clif}");
}

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

#[test]
fn parsed_direct_spawn_with_arguments_transfers_them_through_a_traced_start_environment() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        pub type Fiber<T> { i64 handle, }
        type Counter { i64 value, }
        i64 Entry(i64 delta, Counter counter) { return counter.value + delta; }
        i64 Main() {
            Counter counter = Counter { value: 40_i64 };
            let child = spawn Entry(2_i64, counter);
            return 0_i64;
        }
    ";
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    let plan = lowered
        .artifact
        .aggregate_static_plans
        .iter()
        .find(|plan| plan.allocation_request_symbol.starts_with("__beskid_spawn_arguments_request_"))
        .expect("argument-bearing spawn must materialize a start-environment plan");
    assert_eq!(plan.fields.len(), 2, "one environment field per entry parameter");
    assert_eq!(plan.pointer_map_offsets.len(), 1, "the managed Counter argument is traced");
    assert_eq!(plan.pointer_map_offsets[0], plan.fields[1].field_offset);

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
    assert!(main_clif.contains(&plan.allocation_request_symbol), "{main_clif}");
    assert!(main_clif.contains("fiber_spawn"), "{main_clif}");
    assert!(main_clif.contains("gc_register_root"), "{main_clif}");
    assert!(main_clif.contains("gc_unregister_root"), "{main_clif}");
    assert!(
        !main_clif.contains("Entry#syntax_"),
        "the parent evaluates arguments but never invokes the entry: {main_clif}"
    );
    assert!(trampoline_clif.contains("Entry#syntax_"), "{trampoline_clif}");
    let entry = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Entry#syntax_"))
        .expect("spawned entry is reachable");
    let target_signature = trampoline
        .function
        .dfg
        .ext_funcs
        .values()
        .find(|function| function.name == ExternalName::testcase(entry.name.as_bytes()))
        .map(|function| &trampoline.function.dfg.signatures[function.signature])
        .expect("trampoline invokes the source-owned entry");
    assert_eq!(target_signature.params.len(), 2, "the trampoline passes both loaded arguments");
    let loads = trampoline
        .function
        .layout
        .blocks()
        .flat_map(|block| trampoline.function.layout.block_insts(block))
        .filter(|inst| trampoline.function.dfg.insts[*inst].opcode() == cranelift_codegen::ir::Opcode::Load)
        .count();
    assert_eq!(loads, 2, "the trampoline loads each argument from the environment\n{trampoline_clif}");
}

#[test]
fn parsed_lambda_spawn_with_call_arguments_fails_closed() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        pub type Fiber<T> { i64 handle, }
        i64 Main() { let child = spawn ((i64 value) => value)(7_i64); return 0_i64; }
    ";
    let assembly = parse_production_units(project.path(), &[("Unsupported.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();
    assert_unsupported_closed_failure(assembly, target, isa.as_ref(), &["CalleeArgumentsUnsupported"]);
}

#[test]
fn parsed_zero_capture_lambda_spawn_emits_syntax_owned_entry_and_fiber_dispatch() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        pub type Fiber<T> { i64 handle, }
        i64 Main() { let child = spawn (() => 7_i64); return 0_i64; }
    ";
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    assert_eq!(
        lowered.artifact.functions.len(),
        4,
        "Main, the lambda body entry, the syntax-owned spawn entry, and its trampoline: {:?}",
        lowered.artifact.functions.iter().map(|function| function.name.as_str()).collect::<Vec<_>>()
    );
    assert!(
        lowered.artifact.functions.iter().any(|function| function.name.starts_with("__beskid_lambda_entry_syntax_")),
        "lambda body entry must remain part of the reachable closure"
    );
    let main = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Main#syntax_"))
        .expect("Main artifact");
    let lambda = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("__beskid_spawn_lambda_syntax_"))
        .expect("syntax-owned lambda entry");
    let trampoline = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("__beskid_spawn_entry_syntax_"))
        .expect("syntax-owned spawn trampoline");
    let main_clif = main.function.display().to_string();
    let lambda_clif = lambda.function.display().to_string();
    let trampoline_clif = trampoline.function.display().to_string();
    assert!(main_clif.contains("fiber_spawn"), "{main_clif}");
    assert!(main_clif.contains("func_addr"), "{main_clif}");
    assert!(lambda_clif.contains("iconst.i64 7"), "{lambda_clif}");
    assert!(trampoline_clif.contains("__beskid_spawn_lambda_syntax_"), "{trampoline_clif}");
}

#[test]
fn parsed_capturing_lambda_spawn_allocates_roots_and_dispatches_fiber_entry() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        pub type Fiber<T> { i64 handle, }
        i64 Main() { i64 outer = 41_i64; let child = spawn (() => outer); return 0_i64; }
    ";
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    assert!(
        !lowered.artifact.closure_static_plans.is_empty(),
        "capturing spawn must materialize a generation-safe closure static plan"
    );
    assert_eq!(
        lowered.artifact.closure_static_plans[0].captures.len(),
        1,
        "outer capture must appear in the static plan"
    );
    let main = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Main#syntax_"))
        .expect("Main artifact");
    let lambda = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("__beskid_spawn_lambda_syntax_"))
        .expect("syntax-owned capturing lambda entry");
    let trampoline = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("__beskid_spawn_entry_syntax_"))
        .expect("syntax-owned spawn trampoline");
    let main_clif = main.function.display().to_string();
    let lambda_clif = lambda.function.display().to_string();
    let trampoline_clif = trampoline.function.display().to_string();
    assert!(main_clif.contains("beskid_rt_v5_closure_environment_allocate"), "{main_clif}");
    assert!(main_clif.contains("gc_register_root"), "{main_clif}");
    assert!(main_clif.contains("gc_unregister_root"), "{main_clif}");
    assert!(!main_clif.contains("beskid_rt_v5_closure_environment_root_current"), "{main_clif}");
    assert!(main_clif.contains("fiber_spawn"), "{main_clif}");
    assert!(
        lambda_clif.contains("load") || lambda_clif.contains("ireduce") || lambda_clif.contains("iadd"),
        "lambda entry must read the rooted capture environment: {lambda_clif}"
    );
    assert!(trampoline_clif.contains("__beskid_spawn_lambda_syntax_"), "{trampoline_clif}");
}

#[test]
fn parsed_unit_capturing_lambda_spawn_preserves_zero_return_entry() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        pub type Fiber<T> { i64 handle, }
        unit Serve(i64 listener) { return; }
        i64 Main() { i64 listener = 41_i64; let child = spawn (() => Serve(listener)); return 0_i64; }
    ";
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();
    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    assert_eq!(lowered.artifact.closure_static_plans[0].captures.len(), 1);
    for lambda in lowered.artifact.functions.iter().filter(|function| {
        function.name.starts_with("__beskid_spawn_lambda_syntax_")
            || function.name.starts_with("__beskid_lambda_entry_syntax_")
    }) {
        assert_eq!(lambda.function.signature.params.len(), 1, "capture environment");
        assert!(lambda.function.signature.returns.is_empty(), "unit has no ABI return register");
        let clif = lambda.function.display().to_string();
        assert!(clif.contains("Serve#syntax_"), "unit body keeps its effect: {clif}");
        assert!(clif.contains("load.i64"), "body reads its capture: {clif}");
    }
    let trampoline = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("__beskid_spawn_entry_syntax_"))
        .expect("spawn trampoline");
    assert_eq!(trampoline.function.signature.returns.len(), 1, "transport still returns its owned result box");
    let entry = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("__beskid_spawn_lambda_syntax_"))
        .expect("spawned lambda entry");
    let target_signature = trampoline
        .function
        .dfg
        .ext_funcs
        .values()
        .find(|function| function.name == ExternalName::testcase(entry.name.as_bytes()))
        .map(|function| &trampoline.function.dfg.signatures[function.signature])
        .expect("trampoline invokes the source-owned entry");
    assert!(target_signature.returns.is_empty());
}

#[test]
fn multi_unit_parsed_project_lowers_through_codegen_input_isle_only() {
    let project = tempfile::tempdir().expect("project directory");
    let util_source = "pub i32 Double(i32 value) { return value + value; }";
    let main_source = "
        use Util;
        i32 Main() {
            return Util.Double(21);
        }
    ";
    let assembly =
        parse_production_units(project.path(), &[("Main.bd", "Main", main_source), ("Util.bd", "Util", util_source)]);
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    assert!(lowered.artifact.functions.len() >= 2, "reachable closure must include Main and imported Util.Double");
}

#[test]
fn parsed_extern_contract_call_comparison_lowers_as_an_if_condition() {
    let project = tempfile::tempdir().expect("project directory");
    let source = r#"
        [Extern(Abi:"C", Library:"libc.so.6")]
        pub contract LinuxPthread { i32 sched_yield(); }
        unit Main() {
            if LinuxPthread.sched_yield() != 0 { return; }
            return;
        }
    "#;
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    let main = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Main#syntax_"))
        .expect("Main artifact");
    let clif = main.function.display().to_string();
    assert!(clif.contains("sched_yield"), "the extern contract symbol must remain explicit: {clif}");
    assert!(clif.contains("icmp"), "the imported result must feed the comparison: {clif}");
}

#[test]
fn parsed_project_control_flow_while_break_continue_reaches_verified_clif() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        i32 Main() {
            mut i32 i = 0;
            mut i32 sum = 0;
            while i < 5 {
                i = i + 1;
                if i == 2 { continue; }
                if i == 4 { break; }
                sum = sum + i;
            }
            return sum;
        }
    ";
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    let main = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Main#syntax_"))
        .expect("Main artifact function");
    let clif = main.function.display().to_string();
    assert!(clif.contains("brif"), "expected while/if branching: {clif}");
    assert!(clif.matches("jump").count() >= 2, "expected loop transfer jumps: {clif}");
}

#[test]
fn parsed_project_if_else_reaches_verified_clif() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        i32 Main() {
            i32 value = 3;
            if value < 2 {
                return 1;
            } else {
                return 7;
            }
        }
    ";
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    let main = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Main#syntax_"))
        .expect("Main artifact function");
    let clif = main.function.display().to_string();
    assert!(clif.contains("brif"), "expected if/else branching: {clif}");
}

#[test]
fn parsed_project_range_for_accumulator_reaches_verified_clif_without_hir_fallback() {
    // Mutable assignment inside a parsed range body is governed solely by generation-bound
    // syntax facts. Unsupported operations must fail closed.
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        i32 Main() {
            mut i32 sum = 0;
            for i in range(0, 4) {
                sum = sum + i;
            }
            return sum;
        }
    ";
    let assembly = parse_production_units(project.path(), &[("RangeFor.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();
    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    let main = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Main#syntax_"))
        .expect("Main artifact function");
    let clif = main.function.display().to_string();
    assert!(clif.contains("brif"), "expected range-for branch: {clif}");
    assert!(clif.contains("iadd"), "expected accumulator addition: {clif}");
}

#[test]
fn parsed_project_nested_direct_calls_reach_verified_clif() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        i32 Inner(i32 value) { return value + 1; }
        i32 Mid(i32 value) { return Inner(value) + Inner(value); }
        i32 Main() { return Mid(20); }
    ";
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    assert_eq!(lowered.artifact.functions.len(), 3, "reachable closure must include Main, Mid, and Inner");
    let main = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Main#syntax_"))
        .expect("Main artifact function");
    let mid = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Mid#syntax_"))
        .expect("Mid artifact function");
    assert!(main.function.display().to_string().contains("call"), "Main must call Mid");
    assert!(mid.function.display().to_string().matches("call").count() >= 2, "Mid must nest two Inner calls");
}

#[test]
fn unsupported_lambda_fails_closed_without_legacy_fallback() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        i32 Main() {
            i32 outer = 1;
            let add = (i32 inner) => outer + inner;
            return outer;
        }
    ";
    let assembly = parse_production_units(project.path(), &[("Lambda.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();
    assert_unsupported_closed_failure(assembly, target, isa.as_ref(), &["Lambda.bd", "MissingRuleOrFact"]);
}

#[test]
fn parsed_project_inline_method_reaches_verified_clif_through_production_entrypoint() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        type Point { i32 x, i32 Ping() { return 7; } }
        i32 Main() { return Point { x: 1 }.Ping(); }
    ";
    let assembly = parse_production_units(project.path(), &[("Method.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    assert!(
        lowered.artifact.functions.len() >= 2,
        "reachable closure must include Main and the inline Point.Ping method"
    );
    let main = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Main#syntax_"))
        .expect("Main artifact function");
    assert!(
        main.function.display().to_string().contains("call"),
        "Main must call the inline method through syntax ISLE"
    );
}

#[test]
fn parsed_project_capturing_lambda_keeps_generation_safe_capture_facts_and_fails_closed() {
    let project = tempfile::tempdir().expect("project directory");
    let source_path = project.path().join("Capture.bd");
    let source = "
        i32 Main() {
            i32 outer = 1;
            let apply = (i32 inner) => outer + inner;
            return outer;
        }
    ";
    let assembly = parse_production_units(project.path(), &[("Capture.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    with_db(|db| {
        let generation = assembly.generation;
        let unit = SourceUnitId::new(db, source_path.clone());
        let typed = beskid_queries::build_typed_program(
            db,
            beskid_queries::ProjectSession::new(
                db,
                project.path().to_path_buf(),
                source_path.clone(),
                "App".into(),
                "lock".into(),
            ),
            generation,
            Arc::clone(&assembly),
        )
        .expect("typed capture program");
        assert!(
            typed.runtime_intrinsic_capability.is_none(),
            "ordinary parsed projects must not mint trusted runtime intrinsic authority"
        );
        let root = AstNodeKey { unit, generation, node: AstNodeId(0) };
        let mut pending = vec![root];
        let mut lambda = None;
        while let Some(key) = pending.pop() {
            if matches!(node_kind(db, key), Ok(Some(beskid_queries::IndexedNodeKind::LambdaExpression))) {
                lambda = Some(key);
                break;
            }
            if let Ok(Some(children)) = child_nodes(db, key) {
                pending.extend(children.iter().copied());
            }
        }
        let lambda = lambda.expect("capturing lambda source node");
        let environment =
            closure_environment(db, lambda).expect("capture fact query").expect("generation-safe capture environment");
        assert_eq!(environment.captures.len(), 1, "outer parameter is captured");
        assert_eq!(environment.parameters.len(), 1, "inner lambda parameter");
    });

    assert_unsupported_closed_failure(assembly, target, isa.as_ref(), &["Capture.bd", "MissingRuleOrFact"]);
}

#[test]
fn parsed_project_declared_array_index_assignment_reaches_verified_syntax_isle() {
    // `args` is deliberately a declared array parameter, not an array literal. Index writes must
    // derive their element layout from the generation-bound declaration, rather than relying on
    // literal-only allocation metadata.
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        string[] Store(string[] values, string value) {
            values[0] = value;
            return values;
        }
        string[] Main() {
            string[] values = [\"before\"];
            return Store(values, \"after\");
        }
    ";
    let assembly = parse_production_units(project.path(), &[("ArrayWrite.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    let store = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Store#syntax_"))
        .expect("Store artifact function");
    let clif = store.function.display().to_string();
    assert!(clif.contains("store"), "array assignment must emit a checked element store: {clif}");
    assert!(clif.contains("trap"), "array assignment must retain bounds/null guards: {clif}");
}

#[test]
fn canonical_runtime_production_path_lowers_trusted_intrinsics_to_verified_clif() {
    let (target, isa) = x86_64_target_and_isa();
    let expected_exports = AbiManifestV5::canonical_runtime(target.clone())
        .exports
        .into_iter()
        .map(|entry| entry.symbol)
        .collect::<BTreeSet<_>>();
    let artifact = with_db(|db| lower_canonical_runtime_prepared_syntax(db, target, isa.as_ref()))
        .expect("canonical runtime lowers through TypedProgram → CodegenInput → ISLE");
    let module = JITModule::new(JITBuilder::with_isa(isa.clone(), default_libcall_names()));
    let extern_signatures = collect_validated_extern_signatures(&module, &artifact)
        .expect("manifest externs retain one signature across canonical runtime callsites");
    assert_eq!(extern_signatures["beskid_arch_v5_context_switch"].call_conv, cranelift_codegen::isa::CallConv::SystemV,);
    assert!(!artifact.functions.is_empty(), "canonical Bootstrap must emit at least one verified function");
    assert!(
        artifact.exports.iter().any(|export| export.exported_symbol == "fiber_spawn"),
        "canonical runtime lowering must retain the Scheduler-owned fiber spawn ABI export",
    );
    let actual_exports = artifact.exports.iter().map(|export| export.exported_symbol.clone()).collect::<BTreeSet<_>>();
    // Keep Network's actual source closure in this production lowering gate. Analyzer-only
    // coverage cannot prove mutable assignment facts or emitted control-flow bodies.
    for service in [
        "open",
        "accept",
        "close",
        "read",
        "write",
        "address",
        "options",
        "set_options",
        "shutdown_write",
        "udp_connect",
        "receive",
        "send",
        "dns_resolve",
        "dns_count",
        "dns_address",
        "dns_release",
    ] {
        let symbol = format!("beskid_rt_v5_network_{service}");
        assert!(actual_exports.contains(&symbol), "canonical Network source export `{symbol}` must lower");
    }
    for helper in ["NetworkTableLocked", "NetworkRequestUnlinkLocked", "NetworkPump", "NetworkShutdown"] {
        assert!(
            artifact.functions.iter().any(|function| function.name.starts_with(&format!("{helper}#syntax_"))),
            "canonical Network helper `{helper}` must emit its real body"
        );
    }
    assert!(
        expected_exports.is_subset(&actual_exports),
        "canonical lowering must publish the complete public ABI manifest surface"
    );
    for service in ["str_concat", "gc_collect"] {
        assert!(
            actual_exports.contains(service),
            "canonical lowering must publish source-owned Corelib service adapter `{service}`"
        );
    }
    let imports = artifact.extern_imports.iter().map(|import| import.symbol.as_str()).collect::<BTreeSet<_>>();
    assert!(
        imports.contains("beskid_rt_v5_linux_fs_read_text"),
        "canonical runtime intrinsics must link through the selected target binding"
    );
    assert!(
        !imports.contains("beskid_rt_v5_intrinsic_fs_read_text"),
        "a target-bound intrinsic must not retain its generic manifest symbol"
    );
    assert!(
        !actual_exports.contains("gc_alloc"),
        "generic runtime helper exports must not become ABI roots without a manifest declaration",
    );
    for function in &artifact.functions {
        verify_function(&function.function, isa.flags())
            .unwrap_or_else(|error| panic!("stock CLIF verifier rejected {}: {error}", function.name));
        Context::for_function(function.function.clone())
            .compile(isa.as_ref(), &mut ControlPlane::default())
            .unwrap_or_else(|error| panic!("machine emission rejected {}: {error:?}", function.name));
    }
    for scheduler_entry in ["__beskid_scheduler_fiber_entry", "__beskid_scheduler_return_trampoline"] {
        let imports = artifact.functions.iter().flat_map(|function| function.function.dfg.ext_funcs.values());
        let imports = imports
            .filter(|external| {
                matches!(&external.name, ExternalName::TestCase(name) if name.raw() == scheduler_entry.as_bytes())
            })
            .collect::<Vec<_>>();
        assert!(!imports.is_empty(), "canonical Scheduler must materialize `{scheduler_entry}` as a function address");
        assert!(
            imports.iter().all(|import| !import.colocated),
            "scheduler function addresses must use far relocations because JIT allocations are not range-constrained"
        );
    }
    let fiber_entry = artifact
        .functions
        .iter()
        .find(|function| function.name == "__beskid_scheduler_fiber_entry")
        .expect("generated scheduler fiber entry");
    assert!(
        fiber_entry.function.dfg.ext_funcs.values().any(|external| {
            matches!(&external.name, ExternalName::TestCase(name) if std::str::from_utf8(name.raw())
                .is_ok_and(|name| name.starts_with("FiberDone#syntax_")))
        }),
        "generated scheduler entry must delegate terminal state publication to canonical FiberDone",
    );
    assert!(
        !fiber_entry.function.display().to_string().contains("store"),
        "generated scheduler entry must not duplicate scheduler record state or outcome stores",
    );
    let fiber_entry_imports = fiber_entry
        .function
        .dfg
        .ext_funcs
        .values()
        .filter_map(|external| match &external.name {
            ExternalName::TestCase(name) => std::str::from_utf8(name.raw()).ok(),
            _ => None,
        })
        .collect::<Vec<_>>();
    for scheduler_resume_symbol in
        ["SchedulerContext#syntax_", "SchedulerSetCurrentFiber#syntax_", "ContextSwitch#syntax_"]
    {
        assert!(
            !fiber_entry_imports.iter().any(|name| name.starts_with(scheduler_resume_symbol)),
            "generated scheduler entry must return through the installed scheduler return trampoline instead of \
             importing `{scheduler_resume_symbol}`",
        );
    }
    assert_eq!(fiber_entry.function.signature.call_conv, cranelift_codegen::isa::CallConv::Tail);
    assert!(fiber_entry_imports.contains(&"__beskid_scheduler_return_trampoline"));
    assert!(fiber_entry.function.display().to_string().contains("return_call"));
    assert!(!fiber_entry.function.display().to_string().lines().any(|line| line.trim() == "return"));
    for external in fiber_entry.function.dfg.ext_funcs.values() {
        let ExternalName::TestCase(name) = &external.name else { continue };
        let name = std::str::from_utf8(name.raw()).expect("UTF-8 scheduler symbol");
        let call_conv = fiber_entry.function.dfg.signatures[external.signature].call_conv;
        if name == "__beskid_scheduler_return_trampoline" {
            assert_eq!(call_conv, cranelift_codegen::isa::CallConv::Tail);
        } else {
            assert_eq!(call_conv, cranelift_codegen::isa::CallConv::SystemV);
        }
    }
    let return_trampoline = artifact
        .functions
        .iter()
        .find(|function| function.name == "__beskid_scheduler_return_trampoline")
        .expect("generated scheduler return trampoline");
    let return_trampoline_imports = return_trampoline
        .function
        .dfg
        .ext_funcs
        .values()
        .filter_map(|external| match &external.name {
            ExternalName::TestCase(name) => std::str::from_utf8(name.raw()).ok(),
            _ => None,
        })
        .collect::<Vec<_>>();
    for scheduler_resume_symbol in ["SchedulerContext#syntax_", "SchedulerSetCurrentFiber#syntax_"] {
        assert!(
            return_trampoline_imports.iter().any(|name| name.starts_with(scheduler_resume_symbol)),
            "scheduler return trampoline must remain the sole owner of `{scheduler_resume_symbol}`",
        );
    }
    assert_eq!(return_trampoline.function.signature.call_conv, cranelift_codegen::isa::CallConv::Tail);
    assert!(return_trampoline_imports.contains(&"beskid_arch_v5_context_switch"));
    assert!(!return_trampoline_imports.iter().any(|name| name.starts_with("ContextSwitch#syntax_")));
    assert!(return_trampoline.function.display().to_string().contains("return_call_indirect"));
    assert!(!return_trampoline.function.display().to_string().lines().any(|line| line.trim() == "return"));
    for external in return_trampoline.function.dfg.ext_funcs.values() {
        let ExternalName::TestCase(name) = &external.name else { continue };
        let name = std::str::from_utf8(name.raw()).expect("UTF-8 scheduler symbol");
        let call_conv = return_trampoline.function.dfg.signatures[external.signature].call_conv;
        assert_eq!(call_conv, cranelift_codegen::isa::CallConv::SystemV, "ordinary import `{name}`");
    }
    let context_switch_signatures = artifact
        .functions
        .iter()
        .flat_map(|function| {
            function.function.dfg.ext_funcs.values().filter_map(|external| {
                matches!(&external.name, ExternalName::TestCase(name) if name.raw() == b"beskid_arch_v5_context_switch")
                    .then_some(function.function.dfg.signatures[external.signature].call_conv)
            })
        })
        .collect::<Vec<_>>();
    assert!(!context_switch_signatures.is_empty());
    assert!(context_switch_signatures.iter().all(|call_conv| *call_conv == cranelift_codegen::isa::CallConv::SystemV));
    assert!(
        artifact.functions.iter().any(|function| {
            let clif = function.function.display().to_string();
            clif.contains("iconst") || clif.contains("load") || clif.contains("store")
        }),
        "canonical runtime helpers must emit real CLIF bodies"
    );
}

#[test]
fn parsed_string_literals_do_not_assume_jit_code_and_data_are_colocated() {
    let project = tempfile::tempdir().expect("project directory");
    let assembly = parse_production_units(
        project.path(),
        &[("Main.bd", "Main", "string Main() { return \"range independent\"; }")],
    );
    let (target, isa) = x86_64_target_and_isa();
    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    let literal_symbols =
        lowered.artifact.string_literals.keys().map(|symbol| symbol.as_bytes().to_vec()).collect::<BTreeSet<_>>();
    let literal_references = lowered.artifact.functions.iter().flat_map(|function| {
        function.function.global_values.values().filter_map(|global| match global {
            cranelift_codegen::ir::GlobalValueData::Symbol {
                name: ExternalName::TestCase(name), colocated, ..
            } if literal_symbols.contains(name.raw()) => Some(*colocated),
            _ => None,
        })
    });
    let literal_references = literal_references.collect::<Vec<_>>();
    assert!(!literal_references.is_empty(), "string lowering must retain literal data references");
    assert!(
        literal_references.iter().all(|colocated| !colocated),
        "JIT code and literal data allocations must not assume an AArch64 ADRP-range placement"
    );
}

#[test]
fn production_path_accepts_only_syntax_program_assembly() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        i32 Helper(i32 value) { return value + 1; }
        i32 Main() {
            if Helper(1) > 0 { return Helper(2); }
            return 0;
        }
    ";
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    // The production boundary is ProgramAssembly-only.
    assert_eq!(std::any::type_name_of_val(assembly.as_ref()), "beskid_analysis::projects::assembly::ProgramAssembly");
    let (target, isa) = x86_64_target_and_isa();
    let lowered = lower_verified_entrypoint(Arc::clone(&assembly), target.clone(), isa.as_ref());
    assert!(lowered.artifact.functions.len() >= 2, "direct-call closure through syntax ISLE");

    let public_exports = include_str!("../src/lib.rs");
    assert!(
        public_exports.contains("lower_syntax_assembly_entrypoint"),
        "production codegen must expose the syntax-assembly lowering boundary"
    );
    assert!(
        public_exports.contains("lower_prepared_syntax_entrypoint"),
        "production codegen must expose the prepared-syntax lowering boundary"
    );
}

#[test]
fn public_codegen_surface_names_canonical_syntax_lowering_authority() {
    let public_exports = include_str!("../src/lib.rs");
    let public_prepared_syntax = include_str!("../src/prepared_syntax.rs");
    assert!(
        public_exports.contains("lower_prepared_syntax_module"),
        "module hosts must use the canonical prepared-syntax lowering boundary"
    );
    assert!(
        public_prepared_syntax.contains("CodegenInput::new"),
        "prepared-syntax lowering must construct CodegenInput before emitting ISLE"
    );
    assert!(
        public_prepared_syntax.contains("lower_syntax_program"),
        "prepared-syntax lowering must emit through the syntax ISLE authority"
    );
}
