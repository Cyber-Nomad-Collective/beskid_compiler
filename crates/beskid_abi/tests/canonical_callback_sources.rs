use beskid_abi::runtime_source::{CANONICAL_CALLBACKS_SOURCE_PATH, canonical_runtime_sources};

#[test]
fn canonical_callbacks_source_exists() {
    let sources = canonical_runtime_sources();
    assert!(
        sources.iter().any(|unit| unit.logical_path == CANONICAL_CALLBACKS_SOURCE_PATH),
        "canonical runtime source {CANONICAL_CALLBACKS_SOURCE_PATH} must exist",
    );
}
