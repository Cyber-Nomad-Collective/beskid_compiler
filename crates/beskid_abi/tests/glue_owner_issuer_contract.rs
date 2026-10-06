use beskid_abi::abi_v5::{AbiManifestV5, AbiType, TargetMetadata};

#[test]
fn canonical_owner_issuer_is_versioned_exact_u64_on_every_target() {
    for target in TargetMetadata::supported() {
        let manifest = AbiManifestV5::canonical_runtime(target);
        let export = manifest
            .exports
            .iter()
            .find(|e| e.symbol.as_str() == "beskid_glue_v1_next_identity")
            .expect("canonical provider must own versioned identity export");
        assert!(export.params.is_empty());
        assert_eq!(export.result, AbiType::U64);
    }
}

#[test]
fn issuer_contract_validation_rejects_wrong_signature_and_nonexport_owner() {
    let target = TargetMetadata::for_triple("aarch64-apple-darwin").unwrap();
    let canonical = AbiManifestV5::canonical_runtime(target);
    canonical.validate().expect("exact Glue V1 extension is admitted");
    let index = canonical.exports.iter().position(|f| f.symbol == "beskid_glue_v1_next_identity").unwrap();
    for result in [AbiType::I64, AbiType::U32, AbiType::Pointer, AbiType::Void] {
        let mut invalid = canonical.clone();
        invalid.exports[index].result = result;
        assert!(invalid.validate().is_err());
    }
    let mut invalid = canonical.clone();
    invalid.exports[index].params.push(AbiType::U64);
    assert!(invalid.validate().is_err());
    let mut invalid = canonical.clone();
    invalid.exports[index].noreturn = true;
    assert!(invalid.validate().is_err());
    let mut invalid = canonical.clone();
    let issuer = invalid.exports.remove(index);
    invalid.imports.push(issuer);
    assert!(invalid.validate().is_err(), "runtime provider must own the issuer, not import another authority");
    let mut invalid = canonical;
    invalid.exports[index].symbol = "beskid_glue_v1_guess".into();
    assert!(invalid.validate().is_err());
}
