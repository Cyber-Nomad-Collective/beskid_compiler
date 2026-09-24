//! Array index assignment, canonical runtime intrinsics, and string literal data.

use super::*;

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
