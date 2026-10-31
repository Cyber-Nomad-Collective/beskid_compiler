use beskid_abi::runtime_source::{CANONICAL_COMPOSITION_SOURCE_PATH, canonical_runtime_sources};

#[test]
fn canonical_composition_source_exists() {
    let sources = canonical_runtime_sources();
    assert!(
        sources.iter().any(|unit| unit.logical_path == CANONICAL_COMPOSITION_SOURCE_PATH),
        "canonical runtime source {CANONICAL_COMPOSITION_SOURCE_PATH} must exist",
    );
}
