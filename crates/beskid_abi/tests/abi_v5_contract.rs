use beskid_abi::BESKID_RUNTIME_ABI_VERSION;
use beskid_abi::abi_v5::{
    ABI_V5, AbiFieldLayout, AbiFunction, AbiLayout, AbiManifestV5, AbiType, AssemblyExport,
    CallingConvention, Endianness, ManifestValidationError, PlatformImport, RuntimeAuditMetadata,
    RuntimeIntrinsic, SourceUnit, TargetMetadata, TargetTriple, TrapCode, canonical_source_hash,
};
use beskid_abi::runtime_kit::{
    BuildProfile, RuntimeArtifact, RuntimeArtifacts, RuntimeKitMetadata, RuntimeKitValidationError,
};

fn linux_target() -> TargetMetadata {
    target("x86_64-unknown-linux-gnu")
}

fn target(triple: &str) -> TargetMetadata {
    TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == triple)
        .unwrap()
}

fn function(symbol: &str) -> AbiFunction {
    AbiFunction {
        symbol: symbol.into(),
        param_names: vec!["pointer".into(), "length".into()],
        params: vec![AbiType::Pointer, AbiType::USize],
        result: AbiType::I32,
        noreturn: false,
    }
}

fn layout(name: &str, second_offset: u64) -> AbiLayout {
    AbiLayout {
        name: name.into(),
        size: 16,
        alignment: 8,
        fields: vec![
            AbiFieldLayout {
                name: "pointer".into(),
                offset: 0,
                ty: AbiType::Pointer,
            },
            AbiFieldLayout {
                name: "length".into(),
                offset: second_offset,
                ty: AbiType::USize,
            },
        ],
    }
}

fn valid_manifest() -> AbiManifestV5 {
    let target = linux_target();
    AbiManifestV5 {
        abi_version: ABI_V5,
        trap_exit_status: 101,
        trap_diagnostic: "beskid runtime trap v5".into(),
        imports: vec![function("beskid_rt_v5_alloc")],
        exports: vec![function("beskid_rt_v5_entry")],
        layouts: vec![layout("BeskidSlice", 8)],
        trusted_runtime_package: None,
        trusted_runtime_intrinsics: vec![RuntimeIntrinsic {
            name: "gc_write_barrier".into(),
            symbol: "beskid_rt_v5_intrinsic_gc_write_barrier".into(),
            capability: "runtime.gc".into(),
            param_names: vec!["owner".into(), "value".into()],
            params: vec![AbiType::Pointer, AbiType::Pointer],
            result: AbiType::Void,
            noreturn: false,
        }],
        platform_imports: vec![PlatformImport {
            symbol: "clock_gettime".into(),
            library: "libc".into(),
            param_names: vec!["clock".into(), "timespec".into()],
            params: vec![AbiType::I32, AbiType::Pointer],
            result: AbiType::I32,
            noreturn: false,
        }],
        assembly_exports: AssemblyExport::required_for_target(&target),
        traps: TrapCode::all(),
        target,
    }
}

#[test]
fn abi_version_contract_is_v5() {
    assert_eq!(BESKID_RUNTIME_ABI_VERSION, ABI_V5);
}

#[test]
fn supported_targets_are_little_endian_64_bit_with_target_owned_calling_conventions() {
    for target in TargetMetadata::supported() {
        assert_eq!(target.endianness.as_str(), "little");
        assert_eq!(target.pointer_width, 64);
        target.validate().expect("supported target must validate");
        let json = serde_json::to_string(&target.triple).unwrap();
        assert_eq!(json, format!("\"{}\"", target.triple.as_str()));
        assert_eq!(
            serde_json::from_str::<TargetTriple>(&json).unwrap(),
            target.triple
        );
    }

    let mut unsupported = linux_target();
    unsupported.triple = TargetTriple::new("x86_64-unknown-freebsd");
    assert!(unsupported.validate().is_err());

    let mut wrong_width = linux_target();
    wrong_width.pointer_width = 32;
    assert!(wrong_width.validate().is_err());

    let mut wrong_endian = linux_target();
    wrong_endian.endianness = Endianness::new("big");
    assert!(wrong_endian.validate().is_err());

    let mut wrong_convention = linux_target();
    wrong_convention.calling_convention = CallingConvention::new("windows_x64");
    assert!(wrong_convention.validate().is_err());
}

#[test]
fn manifest_accepts_only_unique_direct_v5_symbols() {
    valid_manifest().validate().expect("valid manifest");

    let mut legacy = valid_manifest();
    legacy.imports[0].symbol = "beskid_runtime_alloc".into();
    assert!(matches!(
        legacy.validate(),
        Err(ManifestValidationError::UnversionedRuntimeSymbol { .. })
    ));

    let mut duplicate = valid_manifest();
    duplicate.exports.push(function("beskid_rt_v5_alloc"));
    assert!(matches!(
        duplicate.validate(),
        Err(ManifestValidationError::DuplicateSymbol { .. })
    ));

    let mut duplicate_intrinsic = valid_manifest();
    duplicate_intrinsic
        .trusted_runtime_intrinsics
        .push(duplicate_intrinsic.trusted_runtime_intrinsics[0].clone());
    assert!(matches!(
        duplicate_intrinsic.validate(),
        Err(ManifestValidationError::DuplicateSymbol { .. })
    ));
}

#[test]
fn manifest_rejects_wrong_assembly_symbol_set_and_invalid_traps() {
    let mut wrong_assembly = valid_manifest();
    wrong_assembly.assembly_exports.pop();
    assert!(matches!(
        wrong_assembly.validate(),
        Err(ManifestValidationError::InvalidAssemblyExports { .. })
    ));

    let mut duplicated_assembly = valid_manifest();
    duplicated_assembly.assembly_exports[1] = duplicated_assembly.assembly_exports[0].clone();
    assert!(matches!(
        duplicated_assembly.validate(),
        Err(ManifestValidationError::InvalidAssemblyExports { .. })
    ));

    let mut incomplete_traps = valid_manifest();
    incomplete_traps.traps.pop();
    assert!(matches!(
        incomplete_traps.validate(),
        Err(ManifestValidationError::InvalidTrapSet { .. })
    ));

    assert!(TrapCode::try_from(0).is_err());
    assert!(TrapCode::try_from(11).is_err());
    for code in 1..=10 {
        assert_eq!(u8::from(TrapCode::try_from(code).unwrap()), code);
    }
    assert_eq!(
        TrapCode::all()
            .iter()
            .map(|trap| (trap.name.as_str(), trap.code))
            .collect::<Vec<_>>(),
        vec![
            ("null_reference", 1),
            ("bounds", 2),
            ("arithmetic_overflow", 3),
            ("invalid_utf8", 4),
            ("out_of_memory", 5),
            ("invalid_or_stale_handle", 6),
            ("scheduler_deadlock", 7),
            ("abi_or_layout_mismatch", 8),
            ("unreachable_or_isle_invariant", 9),
            ("runtime_internal_corruption", 10),
        ]
    );
}

#[test]
fn assembly_contract_rejects_signature_and_preserved_register_drift() {
    let mut reordered = valid_manifest();
    reordered.assembly_exports.reverse();
    reordered
        .validate()
        .expect("assembly export list order is not normative");
    let json = serde_json::to_value(valid_manifest()).unwrap();
    assert_eq!(
        json["assembly_exports"][0]["symbol"],
        "beskid_arch_v5_context_init"
    );

    let mut wrong_params = valid_manifest();
    wrong_params.assembly_exports[0].params = vec![AbiType::I8];
    assert!(matches!(
        wrong_params.validate(),
        Err(ManifestValidationError::InvalidAssemblyExports { .. })
    ));

    let mut missing_register = valid_manifest();
    missing_register.assembly_exports[0]
        .preserved_registers
        .pop();
    assert!(matches!(
        missing_register.validate(),
        Err(ManifestValidationError::InvalidAssemblyExports { .. })
    ));
}

#[test]
fn layout_and_source_hashes_are_canonical_and_sensitive() {
    let mut first = valid_manifest();
    first.layouts.push(layout("BeskidString", 8));
    let mut reordered = first.clone();
    reordered.layouts.reverse();
    assert_eq!(first.layout_hash(), reordered.layout_hash());

    reordered.layouts[0].fields[1].offset = 4;
    assert_ne!(first.layout_hash(), reordered.layout_hash());

    let source_a = SourceUnit {
        logical_path: "Runtime/Gc.bd".into(),
        source: "pub void Collect() {}".into(),
    };
    let source_b = SourceUnit {
        logical_path: "Runtime/Alloc.bd".into(),
        source: "pub ptr Alloc(usize size) {}".into(),
    };
    assert_eq!(
        canonical_source_hash(&[source_a.clone(), source_b.clone()]).unwrap(),
        canonical_source_hash(&[source_b.clone(), source_a.clone()]).unwrap()
    );
    assert_ne!(
        canonical_source_hash(&[source_a.clone(), source_b.clone()]).unwrap(),
        canonical_source_hash(&[
            source_a,
            SourceUnit {
                source: "pub ptr Alloc(usize size) { trap; }".into(),
                ..source_b
            },
        ])
        .unwrap()
    );

    let duplicates = [
        SourceUnit {
            logical_path: "Runtime/Gc.bd".into(),
            source: "first".into(),
        },
        SourceUnit {
            logical_path: "Runtime/Gc.bd".into(),
            source: "second".into(),
        },
    ];
    assert!(matches!(
        canonical_source_hash(&duplicates),
        Err(ManifestValidationError::DuplicateSourcePath { .. })
    ));
}

fn artifact(path: &str) -> RuntimeArtifact {
    RuntimeArtifact {
        relative_path: path.into(),
        sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
    }
}

fn valid_runtime_kit() -> RuntimeKitMetadata {
    runtime_kit(
        linux_target(),
        BuildProfile::Debug,
        RuntimeArtifacts {
            static_library: artifact("static/libbeskid_runtime.a"),
            shared_library: artifact("shared/libbeskid_runtime.so"),
            shared_import_library: None,
        },
    )
}

fn runtime_kit(
    target: TargetMetadata,
    profile: BuildProfile,
    artifacts: RuntimeArtifacts,
) -> RuntimeKitMetadata {
    let abi_contract = AbiManifestV5::canonical_runtime(target.clone());
    let source_hash = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let audit = RuntimeAuditMetadata::for_manifest(&abi_contract, source_hash).unwrap();
    RuntimeKitMetadata {
        schema_version: 1,
        abi_version: ABI_V5,
        target,
        profile,
        layout_hash: abi_contract.layout_hash(),
        source_hash: source_hash.into(),
        artifacts,
        import_allowlist: audit.allowed_imports.clone(),
        export_allowlist: audit.allowed_exports.clone(),
        loader_required_exports: audit.loader_required_exports.clone(),
        abi_contract,
        audit,
    }
}

#[test]
fn runtime_kit_metadata_is_serializable_and_requires_the_exact_target_artifacts() {
    let metadata = valid_runtime_kit();
    metadata.validate().expect("valid runtime kit");
    let json = serde_json::to_string(&metadata).expect("serialize runtime-kit metadata");
    let decoded: RuntimeKitMetadata = serde_json::from_str(&json).expect("deserialize metadata");
    assert_eq!(decoded, metadata);

    let mut wrong_artifact = valid_runtime_kit();
    wrong_artifact.artifacts.shared_library.relative_path = "shared/libbeskid_runtime.dylib".into();
    assert!(matches!(
        wrong_artifact.validate(),
        Err(RuntimeKitValidationError::InvalidArtifactSet { .. })
    ));

    let mut duplicate_export = valid_runtime_kit();
    duplicate_export
        .export_allowlist
        .push("beskid_rt_v5_abi_version".into());
    assert!(matches!(
        duplicate_export.validate(),
        Err(RuntimeKitValidationError::DuplicateAllowlistSymbol { .. })
    ));

    runtime_kit(
        target("aarch64-apple-darwin"),
        BuildProfile::Release,
        RuntimeArtifacts {
            static_library: artifact("static/libbeskid_runtime.a"),
            shared_library: artifact("shared/libbeskid_runtime.dylib"),
            shared_import_library: None,
        },
    )
    .validate()
    .expect("macOS artifact contract");

    let windows = runtime_kit(
        target("x86_64-pc-windows-msvc"),
        BuildProfile::Release,
        RuntimeArtifacts {
            static_library: artifact("static/beskid_runtime.lib"),
            shared_library: artifact("shared/beskid_runtime.dll"),
            shared_import_library: Some(artifact("shared/beskid_runtime_import.lib")),
        },
    );
    windows.validate().expect("Windows artifact contract");
    let mut missing_import_library = windows;
    missing_import_library.artifacts.shared_import_library = None;
    assert!(matches!(
        missing_import_library.validate(),
        Err(RuntimeKitValidationError::InvalidArtifactSet { .. })
    ));
}

#[test]
fn runtime_kit_rejects_non_portable_artifact_paths() {
    for invalid in [
        "/absolute/file",
        "C:/absolute/file",
        "C:\\absolute\\file",
        "\\\\server\\share\\file",
        "../file",
        "./file",
        "shared/../file",
        "shared/./file",
        "shared//file",
        "shared\\file",
    ] {
        let mut metadata = valid_runtime_kit();
        metadata.artifacts.static_library.relative_path = invalid.into();
        assert!(
            matches!(
                metadata.validate(),
                Err(RuntimeKitValidationError::InvalidArtifactPath(_))
            ),
            "portable validation accepted `{invalid}`"
        );
    }
}
