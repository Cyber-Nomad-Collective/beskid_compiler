use beskid_abi::runtime_source::{
    CANONICAL_HUB_SOURCE_PATH, CANONICAL_SCHEDULER_CORE_SOURCE_PATH, CANONICAL_SCHEDULER_STORAGE_SOURCE_PATH,
    canonical_runtime_sources,
};

#[test]
fn canonical_hub_sources_exist() {
    let sources = canonical_runtime_sources();
    for path in
        [CANONICAL_SCHEDULER_CORE_SOURCE_PATH, CANONICAL_SCHEDULER_STORAGE_SOURCE_PATH, CANONICAL_HUB_SOURCE_PATH]
    {
        assert!(sources.iter().any(|unit| unit.logical_path == path), "canonical runtime source {path} must exist",);
    }
}
