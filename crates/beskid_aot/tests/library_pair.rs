use beskid_aot::emit_library_pair;
use beskid_codegen::CodegenArtifact;

#[test]
fn emits_static_and_shared_library_shells_without_runtime_kit() {
    let temp = tempfile::tempdir().expect("tempdir");
    let pair = emit_library_pair(
        CodegenArtifact::default(),
        temp.path().join("out"),
        "runtime_input",
        None,
        Vec::new(),
    )
    .expect("emit pair");
    assert!(pair.static_library.is_file());
    assert!(pair.shared_library.is_file());
    assert!(pair.provenance_symbols.is_empty());
}
