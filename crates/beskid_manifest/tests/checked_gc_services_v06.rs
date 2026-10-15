use beskid_manifest::load_v5_manifest_source;

#[test]
fn checked_gc_services_require_exact_versioned_adapters() {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime_manifest.bsol"),
    ).unwrap();
    load_v5_manifest_source(&source).expect("canonical checked GC manifest");
    for logical in ["gc_same_identity", "gc_try_alloc", "gc_allocation_failure_reason", "gc_try_register_root", "gc_try_root_handle"] {
        let canonical = format!("adapter = \"beskid_rt_v5_{logical}\"");
        assert!(source.contains(&canonical), "canonical adapter exists: {logical}");
        let changed = source.replacen(&canonical, &format!("adapter = \"{logical}\""), 1);
        let error = load_v5_manifest_source(&changed).expect_err("unversioned checked adapter rejected");
        assert!(error.contains("canonical versioned adapter"), "{logical}: {error}");
    }
}
