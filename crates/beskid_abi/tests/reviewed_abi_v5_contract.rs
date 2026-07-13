use beskid_abi::abi_v5::{
    ABI_V5, AbiFunction, AbiManifestV5, AbiType, AssemblyExport, AssemblyRegister, AssemblySymbol,
    CallingConvention, Endianness, ManifestValidationError, SourceUnit, TargetMetadata,
    TargetTriple, TrapCode, canonical_source_hash,
};
use beskid_abi::runtime_kit::{
    BuildProfile, RuntimeArtifact, RuntimeArtifacts, RuntimeKitMetadata, RuntimeKitValidationError,
};

fn target(triple: TargetTriple, calling_convention: CallingConvention) -> TargetMetadata {
    TargetMetadata {
        triple,
        endianness: Endianness::Little,
        pointer_width: 64,
        calling_convention,
    }
}

fn linux_target() -> TargetMetadata {
    target(
        TargetTriple::X86_64UnknownLinuxGnu,
        CallingConvention::SystemV,
    )
}

fn runtime_function(symbol: &str) -> AbiFunction {
    AbiFunction {
        symbol: symbol.into(),
        params: vec![AbiType::Pointer],
        result: AbiType::Void,
    }
}

fn manifest_for(target: TargetMetadata) -> AbiManifestV5 {
    AbiManifestV5 {
        abi_version: ABI_V5,
        imports: vec![runtime_function("beskid_rt_v5_alloc")],
        exports: vec![runtime_function("beskid_rt_v5_entry")],
        layouts: Vec::new(),
        trusted_runtime_intrinsics: Vec::new(),
        platform_imports: Vec::new(),
        assembly_exports: AssemblyExport::required_for_target(&target),
        traps: TrapCode::ALL.to_vec(),
        target,
    }
}

#[test]
fn trap_codes_match_the_approved_meanings_exactly() {
    let expected = [
        (1, TrapCode::NullReference),
        (2, TrapCode::Bounds),
        (3, TrapCode::ArithmeticOverflow),
        (4, TrapCode::InvalidUtf8),
        (5, TrapCode::OutOfMemory),
        (6, TrapCode::InvalidOrStaleHandle),
        (7, TrapCode::SchedulerDeadlock),
        (8, TrapCode::AbiOrLayoutMismatch),
        (9, TrapCode::UnreachableOrIsleInvariant),
        (10, TrapCode::RuntimeInternalCorruption),
    ];
    for (number, meaning) in expected {
        assert_eq!(TrapCode::try_from(number).unwrap(), meaning);
        assert_eq!(u8::from(meaning), number);
    }
}

#[test]
fn target_triples_serialize_as_canonical_rust_triples() {
    let triples = [
        (
            TargetTriple::X86_64UnknownLinuxGnu,
            "x86_64-unknown-linux-gnu",
        ),
        (TargetTriple::Aarch64AppleDarwin, "aarch64-apple-darwin"),
        (TargetTriple::X86_64PcWindowsMsvc, "x86_64-pc-windows-msvc"),
    ];
    for (triple, canonical) in triples {
        let json = serde_json::to_string(&triple).unwrap();
        assert_eq!(json, format!("\"{canonical}\""));
        assert_eq!(serde_json::from_str::<TargetTriple>(&json).unwrap(), triple);
    }
}

#[test]
fn assembly_exports_have_typed_signatures_and_exact_preservation_contracts() {
    for target in TargetMetadata::SUPPORTED {
        let manifest = manifest_for(target.clone());
        manifest.validate().expect("approved assembly contracts");
        assert_eq!(manifest.assembly_exports.len(), 2);
        assert_eq!(
            manifest.assembly_exports[0].symbol,
            AssemblySymbol::ContextInit
        );
        assert_eq!(manifest.assembly_exports[0].result, AbiType::Void);
        assert_eq!(
            manifest.assembly_exports[1].symbol,
            AssemblySymbol::ContextSwitch
        );
        assert_eq!(manifest.assembly_exports[1].result, AbiType::Void);
        assert!(!manifest.assembly_exports[0].preserved_registers.is_empty());
        let json = serde_json::to_value(&manifest).unwrap();
        assert_eq!(
            json["assembly_exports"][0]["symbol"],
            "beskid_arch_v5_context_init"
        );

        let mut wrong = manifest;
        wrong.assembly_exports[0]
            .preserved_registers
            .push(AssemblyRegister::Aarch64X19);
        assert!(matches!(
            wrong.validate(),
            Err(ManifestValidationError::InvalidAssemblyExports { .. })
        ));
    }
}

fn artifact(path: &str) -> RuntimeArtifact {
    RuntimeArtifact {
        relative_path: path.into(),
        sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
    }
}

fn kit(
    target: TargetMetadata,
    profile: BuildProfile,
    artifacts: RuntimeArtifacts,
) -> RuntimeKitMetadata {
    RuntimeKitMetadata {
        schema_version: 1,
        abi_version: ABI_V5,
        target,
        profile,
        layout_hash: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        source_hash: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
        artifacts,
        import_allowlist: vec!["clock_gettime".into()],
        export_allowlist: vec!["beskid_rt_v5_entry".into()],
    }
}

#[test]
fn each_abi_json_has_one_profile_and_the_exact_target_artifacts() {
    let linux = kit(
        linux_target(),
        BuildProfile::Debug,
        RuntimeArtifacts {
            static_library: artifact("static/libbeskid_runtime.a"),
            shared_library: artifact("shared/libbeskid_runtime.so"),
            shared_import_library: None,
        },
    );
    linux.validate().expect("Linux debug abi.json");
    let json = serde_json::to_value(&linux).unwrap();
    assert_eq!(json["profile"], "debug");

    kit(
        target(
            TargetTriple::Aarch64AppleDarwin,
            CallingConvention::AppleAarch64,
        ),
        BuildProfile::Release,
        RuntimeArtifacts {
            static_library: artifact("static/libbeskid_runtime.a"),
            shared_library: artifact("shared/libbeskid_runtime.dylib"),
            shared_import_library: None,
        },
    )
    .validate()
    .expect("macOS release abi.json");

    let windows = kit(
        target(
            TargetTriple::X86_64PcWindowsMsvc,
            CallingConvention::WindowsX64,
        ),
        BuildProfile::Release,
        RuntimeArtifacts {
            static_library: artifact("static/beskid_runtime.lib"),
            shared_library: artifact("shared/beskid_runtime.dll"),
            shared_import_library: Some(artifact("shared/beskid_runtime_import.lib")),
        },
    );
    windows.validate().expect("Windows release abi.json");

    let mut windows_without_import_library = windows;
    windows_without_import_library
        .artifacts
        .shared_import_library = None;
    assert!(matches!(
        windows_without_import_library.validate(),
        Err(RuntimeKitValidationError::InvalidArtifactSet { .. })
    ));

    let mut wrong = linux;
    wrong.artifacts.shared_library.relative_path = "shared/libbeskid_runtime.dylib".into();
    assert!(matches!(
        wrong.validate(),
        Err(RuntimeKitValidationError::InvalidArtifactSet { .. })
    ));
}

#[test]
fn artifact_paths_are_validated_portably_without_host_path_semantics() {
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
        let mut metadata = kit(
            linux_target(),
            BuildProfile::Debug,
            RuntimeArtifacts {
                static_library: artifact("static/libbeskid_runtime.a"),
                shared_library: artifact("shared/libbeskid_runtime.so"),
                shared_import_library: None,
            },
        );
        metadata.artifacts.static_library.relative_path = invalid.into();
        assert!(
            metadata.validate().is_err(),
            "portable validation accepted `{invalid}`"
        );
    }
}

#[test]
fn source_hashing_rejects_duplicate_logical_paths() {
    let duplicate = [
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
        canonical_source_hash(&duplicate),
        Err(ManifestValidationError::DuplicateSourcePath { .. })
    ));
}
