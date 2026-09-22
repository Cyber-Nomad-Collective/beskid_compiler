use beskid_abi::abi_v5::{AbiManifestV5, RuntimeAuditMetadata, TargetMetadata, TargetTriple};
use beskid_abi::runtime_provenance::{RuntimeProvenanceAudit, SymbolList, SymbolListError, parse_symbol_list};

fn target(triple: &str) -> TargetMetadata {
    TargetMetadata::supported()
        .into_iter()
        .find(|candidate| candidate.triple == TargetTriple::from(triple))
        .expect("supported target")
}

const DARWIN_NETWORK_HARDENING_IMPORTS: &[&str] =
    &["__memcpy_chk", "__memset_chk", "__stack_chk_fail", "__stack_chk_guard"];

#[test]
fn darwin_network_hardening_imports_accept_raw_and_normalized_declared_identities() {
    let audit = RuntimeProvenanceAudit::canonical(target("aarch64-apple-darwin")).unwrap();
    for symbol in DARWIN_NETWORK_HARDENING_IMPORTS {
        for spelling in [format!("_{symbol}"), (*symbol).to_owned()] {
            let mut symbols = audit.fixture_symbol_list().unwrap();
            symbols.undefined = vec![spelling.clone()];
            audit.verify_static_archive(&symbols).unwrap_or_else(|error| panic!("{spelling}: {error}"));
            audit.verify_shared(&symbols).unwrap_or_else(|error| panic!("{spelling}: {error}"));
        }
    }
}

#[test]
fn darwin_network_hardening_authority_disappears_when_target_binding_is_removed() {
    let mut manifest = AbiManifestV5::canonical_runtime(target("aarch64-apple-darwin"));
    let mut audit = RuntimeAuditMetadata::for_manifest(&manifest, "test-source").unwrap();
    for intrinsic in &mut manifest.trusted_runtime_intrinsics {
        for binding in &mut intrinsic.target_bindings {
            binding.os_imports.retain(|symbol| !DARWIN_NETWORK_HARDENING_IMPORTS.contains(&symbol.as_str()));
        }
    }
    assert!(
        RuntimeAuditMetadata::for_manifest(&manifest, "test-source").is_err(),
        "a modified binding must not acquire canonical manifest authority"
    );
    audit.allowed_imports.retain(|symbol| !DARWIN_NETWORK_HARDENING_IMPORTS.contains(&symbol.as_str()));
    for symbol in DARWIN_NETWORK_HARDENING_IMPORTS {
        for spelling in [format!("_{symbol}"), (*symbol).to_owned()] {
            assert!(
                audit.audit_linked_runtime_symbol_tables(&[], std::iter::empty(), [spelling.as_str()]).is_err(),
                "undeclared hardening import {spelling} acquired authority"
            );
        }
    }
    for triple in ["x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc"] {
        let audit = RuntimeProvenanceAudit::canonical(target(triple)).unwrap();
        assert!(
            !audit.allowed_imports.iter().any(|symbol| DARWIN_NETWORK_HARDENING_IMPORTS.contains(&symbol.as_str()))
        );
    }
}

#[test]
fn darwin_network_hardening_imports_reject_similar_undeclared_spellings() {
    let audit = RuntimeProvenanceAudit::canonical(target("aarch64-apple-darwin")).unwrap();
    for spelling in ["_memcpy_chk", "____memcpy_chk", "__memcpy_chk_extra", "___stack_chk_guard_extra"] {
        let mut symbols = audit.fixture_symbol_list().unwrap();
        symbols.undefined = vec![spelling.into()];
        assert!(audit.verify_static_archive(&symbols).is_err(), "unexpected import {spelling} acquired authority");
    }
}

#[test]
fn canonical_audit_is_deterministic_and_derives_target_allowlists() {
    let first = RuntimeProvenanceAudit::canonical(target("x86_64-unknown-linux-gnu")).unwrap();
    let second = RuntimeProvenanceAudit::canonical(target("x86_64-unknown-linux-gnu")).unwrap();

    assert_eq!(first, second);
    assert_eq!(first.target, "x86_64-unknown-linux-gnu");
    assert!(first.allowed_exports.contains(&"beskid_rt_v5_trap".into()));
    assert!(
        first.required_exports.contains(&"beskid_trap_message".into()),
        "every generated Corelib service adapter must be registered for JIT lookup"
    );
    assert!(first.allowed_imports.contains(&"mmap".into()));
    assert!(first.forbidden_symbol_families.contains(&"rust".into()));
}

#[test]
fn canonical_audit_serializes_a_stable_machine_readable_contract() {
    let audit = RuntimeProvenanceAudit::canonical(target("x86_64-pc-windows-msvc")).unwrap();
    let json = audit.to_json().unwrap();

    assert_eq!(json, audit.to_json().unwrap());
    assert!(json.contains("\"target\":\"x86_64-pc-windows-msvc\""));
    assert!(json.contains("\"allowedImports\""));
    assert!(json.contains("\"forbiddenSymbolFamilies\""));
}

#[test]
fn windows_tls_and_args_boundary_is_crt_independent() {
    let audit = RuntimeProvenanceAudit::canonical(target("x86_64-pc-windows-msvc")).unwrap();

    for symbol in ["InitOnceExecuteOnce", "TlsAlloc", "TlsGetValue", "TlsSetValue"] {
        assert!(audit.allowed_imports.contains(&symbol.to_owned()), "missing Win32 TLS import {symbol}");
    }
    for symbol in ["_tls_index", "strlen"] {
        assert!(!audit.allowed_imports.contains(&symbol.to_owned()), "unexpected CRT dependency {symbol}");
    }
}

#[test]
fn windows_static_archive_ignores_only_coff_compiler_metadata_definitions() {
    let audit = RuntimeProvenanceAudit::canonical(target("x86_64-pc-windows-msvc")).unwrap();
    let mut symbols = audit.fixture_symbol_list().unwrap();
    symbols.defined.extend([
        "??_C@_0BN@DEMBGLIA@Core?4Args?5handoff?5is?5invalid?$AA@".into(),
        "??_C@_0CE@MKNJCKMH@Core?4Args?5storage?5allocation?5fai@".into(),
        "??_C@_0CJ@KFDPFJCM@Core?4Args?5argument?5index?5is?5out?5@".into(),
        "@feat.00".into(),
    ]);

    audit.verify_static_archive(&symbols).expect("COFF compiler metadata is not a runtime ABI export");

    symbols.defined.push("beskid_rt_v5_undeclared".into());
    let error = audit.verify_static_archive(&symbols).unwrap_err();
    assert!(error.to_string().contains("unexpected"), "unexpected allowlist result: {error}");
}

#[test]
fn portable_fixture_uses_each_targets_native_symbol_spelling() {
    for metadata in TargetMetadata::supported() {
        let audit = RuntimeProvenanceAudit::canonical(metadata).unwrap();
        let fixture = audit.fixture_symbol_list().unwrap();
        audit.verify(&fixture).unwrap();
    }
}

#[test]
fn canonical_audit_accepts_only_manifest_owned_core_args_definitions() {
    for metadata in TargetMetadata::supported() {
        let audit = RuntimeProvenanceAudit::canonical(metadata.clone()).unwrap();
        let mut symbols = audit.fixture_symbol_list().unwrap();
        let target = metadata.triple.as_str();

        symbols.defined.extend(
            beskid_abi::generated::abi_v5_contract::ABI_V5_CORELIB_SERVICE_BINDINGS
                .iter()
                .filter(|binding| binding.target == target)
                .map(|binding| format!("{}{}", metadata.symbol_prefix, binding.implementation)),
        );
        symbols.defined.extend(
            beskid_abi::generated::abi_v5_contract::ABI_V5_CORE_ARGS_ENTRY_ADAPTERS
                .iter()
                .filter(|adapter| adapter.target == target)
                .map(|adapter| format!("{}{}", metadata.symbol_prefix, adapter.handoff)),
        );

        audit.verify(&symbols).unwrap();

        symbols.defined.push(format!("{}beskid_rt_v5_undeclared", metadata.symbol_prefix));
        let error = audit.verify(&symbols).unwrap_err();
        assert!(error.to_string().contains("unexpected"), "unexpected allowlist error: {error}");
    }
}

#[test]
fn darwin_matrix_adapter_symbols_match_the_canonical_import_policy() {
    let audit = RuntimeProvenanceAudit::canonical(target("aarch64-apple-darwin")).unwrap();
    let raw_archive_symbols = SymbolList {
        target: "aarch64-apple-darwin".into(),
        defined: audit.allowed_exports.iter().map(|symbol| format!("_{symbol}")).collect(),
        undefined: vec![
            "__exit".into(),
            "_clock_gettime".into(),
            "_getpid".into(),
            "_mmap".into(),
            "_mprotect".into(),
            "_munmap".into(),
            // A raw Mach-O archive retains the ABI prefix plus the C helper's own underscore.
            "__tlv_bootstrap".into(),
            // The libc errno accessor `__error` gains a third underscore under Mach-O decoration;
            // the matrix adapter strips one, leaving `__error` here.
            "___error".into(),
            "_write".into(),
        ],
    };
    audit.verify_static_archive(&raw_archive_symbols).expect("Darwin raw archive provenance must be canonicalized");

    let symbols = SymbolList {
        target: "aarch64-apple-darwin".into(),
        // `stage-native-runtime-kit-matrix.sh` removes the first Mach-O underscore before it
        // publishes this explicit platform-adapter input. Darwin TLS retains one underscore in
        // `__tlv_bootstrap`, which the object-policy normalizer removes as the Mach-O prefix.
        defined: audit.allowed_exports.clone(),
        undefined: vec![
            "exit".into(),
            "clock_gettime".into(),
            "getpid".into(),
            "mmap".into(),
            "mprotect".into(),
            "munmap".into(),
            "tlv_bootstrap".into(),
            "error".into(),
            "write".into(),
        ],
    };

    audit.verify(&symbols).expect("Darwin matrix provenance must accept its normalized symbol list");

    let mut unexpected = symbols;
    unexpected.undefined.push("malloc".into());
    let error = audit.verify(&unexpected).unwrap_err();
    assert!(error.to_string().contains("unexpected"), "unexpected allowlist error: {error}");
}

#[test]
fn darwin_shared_image_accepts_only_the_macho_stub_binder_toolchain_import() {
    let audit = RuntimeProvenanceAudit::canonical(target("aarch64-apple-darwin")).unwrap();
    let mut symbols = audit.fixture_symbol_list().unwrap();
    symbols.undefined.push("_dyld_stub_binder".into());

    audit.verify_shared(&symbols).expect("Mach-O shared images require dyld's stub binder");
    assert!(
        audit.verify_static_archive(&symbols).is_err(),
        "static Darwin archives must not acquire the shared-loader import"
    );
}

#[test]
fn symbol_list_parser_rejects_target_mismatch() {
    let list = parse_symbol_list("target=x86_64-pc-windows-msvc\ndefined=beskid_rt_v5_abi_version\n").unwrap();
    let audit = RuntimeProvenanceAudit::canonical(target("x86_64-unknown-linux-gnu")).unwrap();

    assert_eq!(
        audit.verify(&list),
        Err(SymbolListError::TargetMismatch {
            expected: "x86_64-unknown-linux-gnu".into(),
            actual: "x86_64-pc-windows-msvc".into(),
        })
    );
}

#[test]
fn symbol_list_rejects_rust_bridge_and_unwind_provenance() {
    let audit = RuntimeProvenanceAudit::canonical(target("aarch64-apple-darwin")).unwrap();
    let list = SymbolList {
        target: "aarch64-apple-darwin".into(),
        defined: audit
            .allowed_exports
            .iter()
            .map(|symbol| format!("_{symbol}"))
            .chain(std::iter::once("_beskid_runtime_bridge_init".into()))
            .collect(),
        undefined: audit
            .allowed_imports
            .iter()
            .map(|symbol| format!("_{symbol}"))
            .chain(std::iter::once("__Unwind_Resume".into()))
            .collect(),
    };

    let error = audit.verify(&list).unwrap_err();
    assert!(error.to_string().contains("forbidden runtime provenance"));
}

#[test]
fn symbol_list_rejects_non_abi_panic_export() {
    let audit = RuntimeProvenanceAudit::canonical(target("x86_64-unknown-linux-gnu")).unwrap();
    let mut list = audit.fixture_symbol_list().unwrap();
    list.defined.push("panic".into());

    let error = audit.verify(&list).unwrap_err();
    assert!(
        error.to_string().contains("forbidden runtime provenance symbol `panic`"),
        "unexpected provenance error: {error}"
    );
}

#[test]
fn static_archive_audit_collapses_per_member_import_references() {
    // `nm -u` walks every archive member, so the Linux platform objects report `mmap` twice: once
    // for the page-allocation intrinsics in platform.S and once for guarded scheduler stacks in
    // platform_host.c. A repeated reference is not an undeclared dependency.
    let audit = RuntimeProvenanceAudit::canonical(target("x86_64-unknown-linux-gnu")).unwrap();
    let mut symbols = audit.fixture_symbol_list().unwrap();
    symbols.undefined.extend(["mmap".to_string(), "munmap".to_string()]);
    symbols.undefined.push("__tls_get_addr".to_string());

    audit.verify_static_archive(&symbols).unwrap();

    // Collapsing repeats must not weaken the allowlist itself.
    let mut undeclared = symbols;
    undeclared.undefined.push("__cxa_atexit".to_string());
    let error = audit.verify_static_archive(&undeclared).unwrap_err();
    assert!(error.to_string().contains("unexpected"), "unexpected allowlist error: {error}");
}

#[test]
fn linux_shared_runtime_allows_only_documented_dynamic_loader_imports() {
    let loader_imports = [
        "_ITM_deregisterTMCloneTable",
        "_ITM_registerTMCloneTable",
        "__cxa_finalize",
        "__gmon_start__",
        // Dynamically resolved ELF TLS is required for a shared runtime loaded with dlopen.
        // The resolver supplies this loader entry point; it is never a runtime-owned API.
        "__tls_get_addr",
    ];
    let linux_audit = RuntimeProvenanceAudit::canonical(target("x86_64-unknown-linux-gnu")).unwrap();
    let mut linux_symbols = linux_audit.fixture_symbol_list().unwrap();
    linux_symbols.undefined.extend(loader_imports.iter().map(ToString::to_string));
    let static_error = linux_audit.verify(&linux_symbols).unwrap_err();
    assert!(static_error.to_string().contains("unexpected"));
    linux_audit.verify_shared(&linux_symbols).unwrap();

    let mut unexpected_linux_symbols = linux_symbols;
    unexpected_linux_symbols.undefined.push("__cxa_atexit".to_string());
    let error = linux_audit.verify_shared(&unexpected_linux_symbols).unwrap_err();
    assert!(error.to_string().contains("unexpected"));

    let darwin_audit = RuntimeProvenanceAudit::canonical(target("aarch64-apple-darwin")).unwrap();
    let mut darwin_symbols = darwin_audit.fixture_symbol_list().unwrap();
    darwin_symbols.undefined.extend(loader_imports.iter().map(ToString::to_string));
    let error = darwin_audit.verify_shared(&darwin_symbols).unwrap_err();
    assert!(error.to_string().contains("unexpected"));
}

#[test]
fn exact_linux_shared_artifact_allows_only_linker_generated_imports() {
    let audit = RuntimeProvenanceAudit::canonical(target("x86_64-unknown-linux-gnu")).unwrap();
    let symbols = SymbolList {
        target: "x86_64-unknown-linux-gnu".into(),
        defined: vec!["beskid_arch_v5_context_init".into(), "beskid_arch_v5_context_switch".into()],
        undefined: ["_ITM_deregisterTMCloneTable", "_ITM_registerTMCloneTable", "__cxa_finalize", "__gmon_start__"]
            .into_iter()
            .map(ToString::to_string)
            .collect(),
    };

    audit.verify_toolchain_imports(&symbols, true).unwrap();

    let mut application_dependency = symbols.clone();
    application_dependency.undefined.push("mmap".into());
    let error = audit.verify_toolchain_imports(&application_dependency, true).unwrap_err();
    assert!(error.to_string().contains("unexpected"), "unexpected allowlist result: {error}");

    let error = audit.verify_toolchain_imports(&symbols, false).unwrap_err();
    assert!(error.to_string().contains("unexpected"), "static archives must not inherit ELF shared imports: {error}");
}
