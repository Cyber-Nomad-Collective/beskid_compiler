fn manifest() -> beskid_manifest::RuntimeManifestV5 {
    beskid_manifest::load_v5_manifest_source(include_str!("../../../runtime_manifest.bsol")).unwrap()
}

#[test]
fn timer_source_binding_reuses_the_export_signature() {
    let manifest = manifest();
    let artifacts = beskid_manifest::generate_v5_artifacts(&manifest).unwrap();
    assert!(artifacts.rust.contains("GeneratedSourceBuiltin { name: \"__timer_sleep_until\", symbol: \"beskid_rt_v5_external_sleep_until\", params: &[\"i64\"], result: \"usize\" }"));
}

#[test]
fn export_backed_source_binding_rejects_signature_drift() {
    let mut manifest = manifest();
    let builtin = manifest.soft_builtins.iter_mut().find(|builtin| builtin.name == "__timer_sleep_until").unwrap();
    builtin.result = "i32".into();
    let error = beskid_manifest::generate_v5_artifacts(&manifest).err().expect("reject signature drift");
    assert!(error.contains("does not match runtime export"), "{error}");
}
