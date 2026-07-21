use beskid_abi::abi_v5::{
    ABI_V5, AbiManifestV5, AbiType, AssemblySymbol, CANONICAL_RUNTIME_PACKAGE_NAME,
    CANONICAL_RUNTIME_PACKAGE_PUBLISHER, ManifestValidationError, RuntimeAuditMetadata,
    RuntimePackageIdentity, TRAP_DIAGNOSTIC_PREFIX, TRAP_EXIT_STATUS, TargetMetadata,
    canonical_runtime_package, render_runtime_asm_include, render_runtime_c_header,
};
use beskid_abi::runtime_kit::{
    BuildProfile, RuntimeArtifact, RuntimeArtifacts, RuntimeKitMetadata, RuntimeKitValidationError,
};

fn supported_targets() -> Vec<TargetMetadata> {
    let targets = TargetMetadata::supported();
    [
        "x86_64-unknown-linux-gnu",
        "aarch64-apple-darwin",
        "x86_64-pc-windows-msvc",
    ]
    .into_iter()
    .map(|triple| {
        targets
            .iter()
            .find(|target| target.triple.as_str() == triple)
            .unwrap()
            .clone()
    })
    .collect()
}

#[test]
fn canonical_contract_has_the_exact_lifecycle_closure_and_trap_exports() {
    let manifest = AbiManifestV5::canonical_runtime(supported_targets()[0].clone());
    manifest.validate().expect("canonical runtime contract");

    let actual = manifest
        .exports
        .iter()
        .map(|entry| (entry.symbol.as_str(), entry.params.as_slice(), entry.result))
        .collect::<Vec<_>>();
    assert_eq!(
        actual,
        vec![
            (
                "beskid_library_attach_v5",
                &[AbiType::Pointer][..],
                AbiType::I32,
            ),
            (
                "beskid_library_detach_v5",
                &[AbiType::Pointer][..],
                AbiType::Void,
            ),
            ("beskid_rt_v5_abi_version", &[][..], AbiType::U32),
            (
                "beskid_rt_v5_closure_capture_store",
                &[
                    AbiType::Pointer,
                    AbiType::Pointer,
                    AbiType::USize,
                    AbiType::Pointer,
                ][..],
                AbiType::U8,
            ),
            (
                "beskid_rt_v5_closure_environment_allocate",
                &[AbiType::Pointer][..],
                AbiType::Pointer,
            ),
            (
                "beskid_rt_v5_closure_environment_root",
                &[AbiType::Pointer, AbiType::USize, AbiType::Pointer][..],
                AbiType::U8,
            ),
            (
                "beskid_rt_v5_process_init",
                &[AbiType::Pointer][..],
                AbiType::Pointer,
            ),
            (
                "beskid_rt_v5_process_shutdown",
                &[AbiType::Pointer][..],
                AbiType::Void,
            ),
            (
                "beskid_rt_v5_thread_attach",
                &[AbiType::Pointer][..],
                AbiType::Pointer,
            ),
            (
                "beskid_rt_v5_thread_detach",
                &[AbiType::Pointer][..],
                AbiType::Void,
            ),
            (
                "beskid_rt_v5_trap",
                &[AbiType::U8, AbiType::Pointer, AbiType::USize][..],
                AbiType::Void,
            ),
        ]
    );
    assert_eq!(TRAP_EXIT_STATUS, 101);
    assert_eq!(TRAP_DIAGNOSTIC_PREFIX, "beskid runtime trap v5");
    let trap = manifest
        .exports
        .iter()
        .find(|entry| entry.symbol == "beskid_rt_v5_trap")
        .unwrap();
    assert!(trap.noreturn);
}

#[test]
fn trusted_intrinsics_are_typed_and_owned_only_by_the_canonical_package() {
    let manifest = AbiManifestV5::canonical_runtime(supported_targets()[0].clone());
    let package = canonical_runtime_package();
    assert_eq!(package.publisher(), CANONICAL_RUNTIME_PACKAGE_PUBLISHER);
    assert_eq!(package.name(), CANONICAL_RUNTIME_PACKAGE_NAME);
    assert_eq!(package.abi_version(), ABI_V5);
    let names = manifest
        .trusted_runtime_intrinsics
        .iter()
        .map(|intrinsic| intrinsic.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names.len(), 15);
    assert!(names.contains(&"pointer_add"));
    assert!(names.contains(&"raw_word_load"));
    assert!(names.contains(&"system_allocate"));
    assert!(names.contains(&"tls_get"));
    assert!(names.contains(&"trap"));
    assert!(manifest.intrinsic_metadata("pointer_add").is_some());

    let mut unauthorized = manifest.clone();
    unauthorized.trusted_runtime_package = Some(
        serde_json::from_str::<RuntimePackageIdentity>(
            r#"{"publisher":"beskid-lang.org","name":"user-runtime-lookalike","abi_version":5}"#,
        )
        .unwrap(),
    );
    assert!(matches!(
        unauthorized.validate(),
        Err(ManifestValidationError::UnauthorizedRuntimePackage { .. })
    ));
}

#[test]
fn runtime_provenance_allows_intrinsics_without_making_them_loader_requirements() {
    let manifest = AbiManifestV5::canonical_runtime(supported_targets()[0].clone());
    let audit = RuntimeAuditMetadata::for_manifest(
        &manifest,
        "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
    )
    .expect("canonical audit metadata");

    assert!(
        audit
            .allowed_exports
            .contains(&"beskid_rt_v5_process_init".into())
    );
    assert!(
        audit
            .allowed_exports
            .contains(&"beskid_rt_v5_intrinsic_memory_compare".into())
    );
    assert!(
        audit
            .loader_required_exports
            .contains(&"beskid_rt_v5_process_init".into())
    );
    assert!(
        !audit
            .loader_required_exports
            .contains(&"beskid_rt_v5_intrinsic_memory_compare".into())
    );
}

#[test]
fn canonical_layouts_freeze_common_and_target_context_offsets() {
    let expected_contexts = [
        ("BeskidArchContextX86_64SysV", 64, 16, "rip", 56),
        ("BeskidArchContextAarch64Darwin", 176, 16, "d15", 168),
        ("BeskidArchContextX86_64Windows", 240, 16, "xmm15", 224),
    ];
    for (target, expected) in supported_targets().into_iter().zip(expected_contexts) {
        let is_windows = target.triple.as_str() == "x86_64-pc-windows-msvc";
        let manifest = AbiManifestV5::canonical_runtime(target);
        manifest.validate().expect("canonical target layout");
        let names = manifest
            .layouts
            .iter()
            .map(|layout| layout.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "BeskidAllocationRequest",
                "BeskidHandle",
                "BeskidObjectHeader",
                "BeskidRootFrame",
                "BeskidRootSlot",
                "BeskidRuntimeState",
                "BeskidTlsState",
                "BeskidTypeDescriptor",
                expected.0,
            ]
        );
        let context = manifest
            .layouts
            .iter()
            .find(|layout| layout.name == expected.0)
            .unwrap();
        assert_eq!((context.size, context.alignment), (expected.1, expected.2));
        assert_eq!(
            context
                .fields
                .iter()
                .find(|field| field.name == expected.3)
                .unwrap()
                .offset,
            expected.4
        );
        if is_windows {
            assert_eq!(
                context
                    .fields
                    .iter()
                    .find(|field| field.name == "xmm6")
                    .unwrap()
                    .ty,
                AbiType::V128
            );
        }

        let object = manifest
            .layouts
            .iter()
            .find(|layout| layout.name == "BeskidObjectHeader")
            .unwrap();
        assert_eq!((object.size, object.alignment), (16, 8));
        assert_eq!(object.fields[0].name, "descriptor");
        assert_eq!(object.fields[0].offset, 0);
        assert_eq!(object.fields[1].name, "gc_word");
        assert_eq!(object.fields[1].offset, 8);

        let tls = manifest
            .layouts
            .iter()
            .find(|layout| layout.name == "BeskidTlsState")
            .unwrap();
        assert_eq!((tls.size, tls.alignment), (32, 8));
        assert_eq!(
            tls.fields
                .iter()
                .map(|field| (field.name.as_str(), field.offset))
                .collect::<Vec<_>>(),
            vec![
                ("runtime", 0),
                ("root_frame", 8),
                ("current_fiber", 16),
                ("attach_depth", 24),
            ]
        );

        let runtime = manifest
            .layouts
            .iter()
            .find(|layout| layout.name == "BeskidRuntimeState")
            .unwrap();
        assert_eq!((runtime.size, runtime.alignment), (64, 8));
        assert_eq!(
            runtime
                .fields
                .iter()
                .find(|field| field.name == "abi_version")
                .unwrap()
                .offset,
            0
        );
        assert_eq!(
            runtime
                .fields
                .iter()
                .find(|field| field.name == "current_thread")
                .unwrap()
                .offset,
            8
        );
        assert_eq!(
            runtime
                .fields
                .iter()
                .find(|field| field.name == "root_frame")
                .unwrap()
                .offset,
            40,
            "RuntimeState.root_frame must stay distinct from TlsState.root_frame@8"
        );
    }
}

#[test]
fn target_system_imports_are_exact_and_unknown_contracts_are_rejected() {
    let expected = [
        vec!["_exit", "mmap", "munmap", "write"],
        vec!["_exit", "mmap", "munmap", "write"],
        vec![
            "ExitProcess",
            "GetStdHandle",
            "VirtualAlloc",
            "VirtualFree",
            "WriteFile",
        ],
    ];
    let expected_libraries = ["libc", "libSystem", "kernel32"];
    for ((target, expected_symbols), expected_library) in supported_targets()
        .into_iter()
        .zip(expected)
        .zip(expected_libraries)
    {
        let mut manifest = AbiManifestV5::canonical_runtime(target);
        assert_eq!(
            manifest
                .platform_imports
                .iter()
                .map(|entry| entry.symbol.as_str())
                .collect::<Vec<_>>(),
            expected_symbols
        );
        assert!(
            manifest
                .platform_imports
                .iter()
                .all(|entry| entry.library == expected_library)
        );
        manifest.platform_imports.pop();
        assert!(matches!(
            manifest.validate(),
            Err(ManifestValidationError::InvalidPlatformImportSet { .. })
        ));
    }

    let mut duplicate = AbiManifestV5::canonical_runtime(supported_targets()[0].clone());
    duplicate.layouts.push(duplicate.layouts[0].clone());
    assert!(matches!(
        duplicate.validate(),
        Err(ManifestValidationError::DuplicateLayout { .. })
    ));

    let mut unknown = AbiManifestV5::canonical_runtime(supported_targets()[0].clone());
    unknown.exports[0].symbol = "beskid_rt_v5_surprise".into();
    assert!(matches!(
        unknown.validate(),
        Err(ManifestValidationError::InvalidRuntimeExportSet { .. })
    ));
}

#[test]
fn generated_headers_are_deterministic_fresh_and_include_contract_constants() {
    for target in supported_targets() {
        let manifest = AbiManifestV5::canonical_runtime(target);
        let c_header = render_runtime_c_header(&manifest).unwrap();
        let asm_include = render_runtime_asm_include(&manifest).unwrap();
        assert_eq!(c_header, render_runtime_c_header(&manifest).unwrap());
        assert_eq!(asm_include, render_runtime_asm_include(&manifest).unwrap());
        assert!(c_header.contains("#define BESKID_RUNTIME_ABI_VERSION 5"));
        assert!(c_header.contains("#define BESKID_TRAP_EXIT_STATUS 101"));
        assert!(
            c_header
                .lines()
                .any(|line| line == "#define BESKID_OBJECT_HEADER_DESCRIPTOR_OFFSET 0")
        );
        assert!(c_header.contains("beskid_rt_v5_process_init"));
        assert!(
            c_header.contains(
                "void beskid_arch_v5_context_init(void * context, void * stack_top, void * entry, void * argument, void * return_trampoline);"
            )
        );
        assert!(c_header.contains("void beskid_arch_v5_context_switch(void * from, void * to);"));
        match manifest.target.triple.as_str() {
            "x86_64-unknown-linux-gnu" => {
                assert!(
                    asm_include.contains("BESKID_X86_64_UNKNOWN_LINUX_GNU_STACK_ALIGNMENT = 16")
                );
                assert!(!asm_include.contains("AARCH64"));
                assert!(!asm_include.contains("WINDOWS"));
            }
            "aarch64-apple-darwin" => {
                assert!(asm_include.contains("BESKID_AARCH64_APPLE_DARWIN_CONTEXT_SIZE = 176"));
                assert!(!asm_include.contains("X86_64"));
                assert!(!asm_include.contains("stack+40"));
            }
            "x86_64-pc-windows-msvc" => {
                assert!(asm_include.contains("BESKID_X86_64_PC_WINDOWS_MSVC_SHADOW_SPACE EQU 32"));
                assert!(asm_include.contains(
                    "BESKID_CONTEXT_INIT_RETURN_TRAMPOLINE_STACK_OPERAND TEXTEQU <[rsp + 40]>"
                ));
                assert!(!asm_include.contains("AARCH64"));
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn assembly_init_signature_uses_stack_top_not_a_hardcoded_size() {
    for target in supported_targets() {
        let manifest = AbiManifestV5::canonical_runtime(target);
        let init = manifest
            .assembly_exports
            .iter()
            .find(|entry| entry.symbol == AssemblySymbol::new("beskid_arch_v5_context_init"))
            .unwrap();
        assert_eq!(init.params, vec![AbiType::Pointer; 5]);
        assert_eq!(init.result, AbiType::Void);
    }
}

fn artifact(path: &str) -> RuntimeArtifact {
    RuntimeArtifact {
        relative_path: path.into(),
        sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
    }
}

fn runtime_kit() -> RuntimeKitMetadata {
    let abi_contract = AbiManifestV5::canonical_runtime(supported_targets()[0].clone());
    let source_hash = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let audit = RuntimeAuditMetadata::for_manifest(&abi_contract, source_hash).unwrap();
    RuntimeKitMetadata {
        schema_version: 1,
        abi_version: ABI_V5,
        target: abi_contract.target.clone(),
        profile: BuildProfile::Debug,
        layout_hash: abi_contract.layout_hash(),
        source_hash: source_hash.into(),
        artifacts: RuntimeArtifacts {
            static_library: artifact("static/libbeskid_runtime.a"),
            shared_library: artifact("shared/libbeskid_runtime.so"),
            shared_import_library: None,
        },
        import_allowlist: audit.allowed_imports.clone(),
        export_allowlist: audit.allowed_exports.clone(),
        loader_required_exports: audit.loader_required_exports.clone(),
        abi_contract,
        audit,
    }
}

#[test]
fn runtime_kit_abi_json_embeds_the_single_contract_and_generated_audit_metadata() {
    let metadata = runtime_kit();
    metadata.validate().expect("coherent runtime kit");
    assert_eq!(
        metadata.canonical_abi_json().unwrap(),
        metadata.canonical_abi_json().unwrap()
    );
    let json = serde_json::to_value(&metadata).unwrap();
    assert_eq!(json["abi_contract"]["abi_version"], ABI_V5);
    assert_eq!(
        json["abi_contract"]["trusted_runtime_package"]["name"],
        CANONICAL_RUNTIME_PACKAGE_NAME
    );
    assert_eq!(json["audit"]["layout_hash"], metadata.layout_hash);
    assert_eq!(json["audit"]["runtime_source_hash"], metadata.source_hash);
    assert!(
        metadata
            .audit
            .forbidden_rust_symbols
            .contains(&"rust".into())
    );
    assert!(
        metadata
            .audit
            .allowed_exports
            .contains(&"beskid_arch_v5_context_switch".into())
    );

    let mut layout_drift = metadata.clone();
    layout_drift.layout_hash =
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into();
    assert!(matches!(
        layout_drift.validate(),
        Err(RuntimeKitValidationError::ContractLayoutHashMismatch { .. })
    ));

    let mut export_drift = metadata.clone();
    export_drift.export_allowlist.pop();
    assert!(matches!(
        export_drift.validate(),
        Err(RuntimeKitValidationError::ContractAuditMismatch { .. })
    ));

    let mut source_drift = metadata;
    source_drift.audit.runtime_source_hash =
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".into();
    assert!(matches!(
        source_drift.validate(),
        Err(RuntimeKitValidationError::ContractSourceHashMismatch { .. })
    ));

    let mut missing_trust = runtime_kit();
    missing_trust.abi_contract.trusted_runtime_package = None;
    assert!(matches!(
        missing_trust.validate(),
        Err(RuntimeKitValidationError::InvalidAbiContract)
    ));
}

#[test]
fn audit_metadata_rejects_unknown_duplicate_and_rust_provenance_contracts() {
    let manifest = AbiManifestV5::canonical_runtime(supported_targets()[0].clone());
    let audit = RuntimeAuditMetadata::for_manifest(
        &manifest,
        "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
    )
    .unwrap();
    audit.validate(&manifest).expect("generated audit metadata");

    let mut duplicate = audit.clone();
    duplicate
        .allowed_imports
        .push(duplicate.allowed_imports[0].clone());
    assert!(duplicate.validate(&manifest).is_err());

    let mut unknown = audit.clone();
    unknown.allowed_imports.push("mystery_allocator".into());
    assert!(unknown.validate(&manifest).is_err());

    let elf_undefined = audit
        .allowed_imports
        .iter()
        .map(|symbol| format!("{symbol}@GLIBC_2.2.5"))
        .collect::<Vec<_>>();
    assert!(
        audit
            .audit_object_symbol_tables(
                audit.allowed_exports.iter().map(String::as_str),
                elf_undefined.iter().map(String::as_str),
            )
            .is_ok()
    );

    let mut missing_rust_guard = audit;
    missing_rust_guard
        .forbidden_rust_symbols
        .retain(|symbol| symbol != "rust");
    assert!(missing_rust_guard.validate(&manifest).is_err());

    let macho = AbiManifestV5::canonical_runtime(supported_targets()[1].clone());
    let macho_audit = RuntimeAuditMetadata::for_manifest(
        &macho,
        "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
    )
    .unwrap();
    let macho_defined = macho_audit
        .allowed_exports
        .iter()
        .map(|symbol| format!("_{symbol}"))
        .collect::<Vec<_>>();
    let macho_undefined = macho_audit
        .allowed_imports
        .iter()
        .map(|symbol| format!("_{symbol}"))
        .collect::<Vec<_>>();
    assert!(
        macho_audit
            .audit_object_symbol_tables(
                macho_defined.iter().map(String::as_str),
                macho_undefined.iter().map(String::as_str),
            )
            .is_ok()
    );
    for forbidden in [
        "___rust_alloc",
        "_core::panicking::panic_fmt",
        "_rust_eh_personality",
        "_abfall_switch",
        "__RNvCs1234_4core9panicking9panic_fmt",
        "__ZN4core9panicking9panic_fmt17h0123456789abcdefE",
    ] {
        let mut defined = macho_defined.clone();
        defined.push(forbidden.into());
        assert!(
            macho_audit
                .audit_object_symbol_tables(
                    defined.iter().map(String::as_str),
                    macho_undefined.iter().map(String::as_str),
                )
                .is_err()
        );
    }

    let mut missing = macho_defined.clone();
    missing.pop();
    assert!(
        macho_audit
            .audit_object_symbol_tables(
                missing.iter().map(String::as_str),
                macho_undefined.iter().map(String::as_str),
            )
            .is_err()
    );

    let windows = AbiManifestV5::canonical_runtime(supported_targets()[2].clone());
    let windows_audit = RuntimeAuditMetadata::for_manifest(
        &windows,
        "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
    )
    .unwrap();
    let windows_undefined = windows_audit
        .allowed_imports
        .iter()
        .map(|symbol| format!("__imp_{symbol}"))
        .collect::<Vec<_>>();
    assert!(
        windows_audit
            .audit_object_symbol_tables(
                windows_audit.allowed_exports.iter().map(String::as_str),
                windows_undefined.iter().map(String::as_str),
            )
            .is_ok()
    );
}

#[test]
fn abi_json_rejects_unknown_fields() {
    let metadata = runtime_kit();
    let mut json = serde_json::to_value(&metadata).unwrap();
    json.as_object_mut()
        .unwrap()
        .insert("surprise".into(), serde_json::Value::Bool(true));
    assert!(serde_json::from_value::<RuntimeKitMetadata>(json).is_err());

    let mut contract = serde_json::to_value(&metadata.abi_contract).unwrap();
    contract
        .as_object_mut()
        .unwrap()
        .insert("surprise".into(), serde_json::Value::Bool(true));
    assert!(serde_json::from_value::<AbiManifestV5>(contract).is_err());
}
