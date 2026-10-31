use beskid_abi::runtime_source::{CANONICAL_COLLECTIONS_SOURCE_PATH, canonical_runtime_sources};

#[test]
fn canonical_collections_source_exists() {
    let sources = canonical_runtime_sources();
    assert!(
        sources.iter().any(|unit| unit.logical_path == CANONICAL_COLLECTIONS_SOURCE_PATH),
        "canonical runtime source {CANONICAL_COLLECTIONS_SOURCE_PATH} must exist",
    );
}
