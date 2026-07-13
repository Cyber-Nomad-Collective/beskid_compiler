use beskid_abi::BESKID_RUNTIME_ABI_VERSION;
use beskid_abi::abi_v5::{
    ABI_V5, APPROVED_ASSEMBLY_SYMBOLS, AbiFieldLayout, AbiFunction, AbiLayout, AbiManifestV5,
    AbiType, CallingConvention, Endianness, ManifestValidationError, PlatformImport,
    RuntimeIntrinsic, SourceUnit, TargetMetadata, TargetTriple, TrapCode, canonical_source_hash,
};
use beskid_abi::runtime_kit::{
    ArtifactLinkage, BuildProfile, RuntimeArtifact, RuntimeKitMetadata, RuntimeKitValidationError,
};

fn linux_target() -> TargetMetadata {
    TargetMetadata {
        triple: TargetTriple::X86_64UnknownLinuxGnu,
        endianness: Endianness::Little,
        pointer_width: 64,
        calling_convention: CallingConvention::SystemV,
    }
}

fn function(symbol: &str) -> AbiFunction {
    AbiFunction {
        symbol: symbol.into(),
        params: vec![AbiType::Pointer, AbiType::USize],
        result: AbiType::I32,
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
    AbiManifestV5 {
        abi_version: ABI_V5,
        target: linux_target(),
        imports: vec![function("beskid_rt_v5_alloc")],
        exports: vec![function("beskid_rt_v5_entry")],
        layouts: vec![layout("BeskidSlice", 8)],
        trusted_runtime_intrinsics: vec![RuntimeIntrinsic {
            name: "gc_write_barrier".into(),
            capability: "runtime.gc".into(),
            params: vec![AbiType::Pointer, AbiType::Pointer],
            result: AbiType::Void,
        }],
        platform_imports: vec![PlatformImport {
            symbol: "clock_gettime".into(),
            library: "libc".into(),
            params: vec![AbiType::I32, AbiType::Pointer],
            result: AbiType::I32,
        }],
        assembly_symbols: APPROVED_ASSEMBLY_SYMBOLS.map(str::to_owned).to_vec(),
        traps: TrapCode::ALL.to_vec(),
    }
}

#[test]
fn abi_version_contract_is_v5() {
    assert_eq!(BESKID_RUNTIME_ABI_VERSION, ABI_V5);
}

#[test]
fn supported_targets_are_little_endian_64_bit_with_target_owned_calling_conventions() {
    for target in TargetMetadata::SUPPORTED {
        assert_eq!(target.endianness, Endianness::Little);
        assert_eq!(target.pointer_width, 64);
        target.validate().expect("supported target must validate");
    }

    let mut unsupported = linux_target();
    unsupported.triple = TargetTriple::Other("x86_64-unknown-freebsd".into());
    assert!(unsupported.validate().is_err());

    let mut wrong_width = linux_target();
    wrong_width.pointer_width = 32;
    assert!(wrong_width.validate().is_err());

    let mut wrong_endian = linux_target();
    wrong_endian.endianness = Endianness::Big;
    assert!(wrong_endian.validate().is_err());

    let mut wrong_convention = linux_target();
    wrong_convention.calling_convention = CallingConvention::WindowsX64;
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
}

#[test]
fn manifest_rejects_wrong_assembly_symbol_set_and_invalid_traps() {
    let mut wrong_assembly = valid_manifest();
    wrong_assembly.assembly_symbols.pop();
    assert!(matches!(
        wrong_assembly.validate(),
        Err(ManifestValidationError::InvalidAssemblySymbols { .. })
    ));

    let mut duplicated_assembly = valid_manifest();
    duplicated_assembly.assembly_symbols[1] = duplicated_assembly.assembly_symbols[0].clone();
    assert!(matches!(
        duplicated_assembly.validate(),
        Err(ManifestValidationError::InvalidAssemblySymbols { .. })
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
        canonical_source_hash(&[source_a.clone(), source_b.clone()]),
        canonical_source_hash(&[source_b.clone(), source_a.clone()])
    );
    assert_ne!(
        canonical_source_hash(&[source_a.clone(), source_b.clone()]),
        canonical_source_hash(&[
            source_a,
            SourceUnit {
                source: "pub ptr Alloc(usize size) { trap; }".into(),
                ..source_b
            },
        ])
    );
}

fn artifact(profile: BuildProfile, linkage: ArtifactLinkage, path: &str) -> RuntimeArtifact {
    RuntimeArtifact {
        profile,
        linkage,
        relative_path: path.into(),
        sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
    }
}

fn valid_runtime_kit() -> RuntimeKitMetadata {
    RuntimeKitMetadata {
        schema_version: 1,
        abi_version: ABI_V5,
        target: linux_target(),
        layout_hash: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        source_hash: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
        artifacts: vec![
            artifact(
                BuildProfile::Debug,
                ArtifactLinkage::Static,
                "debug/libbeskid_rt.a",
            ),
            artifact(
                BuildProfile::Debug,
                ArtifactLinkage::Shared,
                "debug/libbeskid_rt.so",
            ),
            artifact(
                BuildProfile::Release,
                ArtifactLinkage::Static,
                "release/libbeskid_rt.a",
            ),
            artifact(
                BuildProfile::Release,
                ArtifactLinkage::Shared,
                "release/libbeskid_rt.so",
            ),
        ],
        import_allowlist: vec!["clock_gettime".into()],
        export_allowlist: vec!["beskid_rt_v5_entry".into()],
    }
}

#[test]
fn runtime_kit_metadata_is_serializable_and_requires_the_complete_artifact_matrix() {
    let metadata = valid_runtime_kit();
    metadata.validate().expect("valid runtime kit");
    let json = serde_json::to_string(&metadata).expect("serialize runtime-kit metadata");
    let decoded: RuntimeKitMetadata = serde_json::from_str(&json).expect("deserialize metadata");
    assert_eq!(decoded, metadata);

    let mut missing = valid_runtime_kit();
    missing.artifacts.pop();
    assert!(matches!(
        missing.validate(),
        Err(RuntimeKitValidationError::InvalidArtifactMatrix { .. })
    ));

    let mut duplicate_export = valid_runtime_kit();
    duplicate_export
        .export_allowlist
        .push("beskid_rt_v5_entry".into());
    assert!(matches!(
        duplicate_export.validate(),
        Err(RuntimeKitValidationError::DuplicateAllowlistSymbol { .. })
    ));
}
