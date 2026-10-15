use beskid_manifest::load_v5_manifest_source;

#[test]
fn v06_distinct_bsol_numbers_preserve_canonical_runtime_manifest_semantics() {
    let manifest = load_v5_manifest_source(include_str!("../../../runtime_manifest.bsol"))
        .expect("ABI manifest integer fields remain valid with distinct BSOL numeric nodes");
    assert_eq!(manifest.meta.abi_version, 5);
    let descriptor = manifest.layouts.iter().find(|layout| layout.name == "BeskidTypeDescriptor").unwrap();
    assert_eq!((descriptor.size, descriptor.alignment), (40, 8));
    let runtime = manifest.layouts.iter().find(|layout| layout.name == "BeskidRuntimeState").unwrap();
    assert_eq!((runtime.size, runtime.alignment), (64, 8));
}

#[test]
fn v06_abi_integer_fields_reject_fractional_and_negative_numeric_nodes() {
    let source = include_str!("../../../runtime_manifest.bsol");
    for invalid in ["5.5", "-5"] {
        let changed = source.replacen("abi_version = 5", &format!("abi_version = {invalid}"), 1);
        assert_ne!(changed, source, "fixture replacement must exercise the numeric field");
        assert!(load_v5_manifest_source(&changed).is_err());
    }
}
