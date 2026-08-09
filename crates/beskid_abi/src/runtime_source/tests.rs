use super::*;
use crate::abi_v5::AbiManifestV5;

#[test]
fn canonical_args_source_exposes_exactly_count_and_get_services() {
    let source = canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == CANONICAL_CORELIB_ARGS_SOURCE_PATH)
        .expect("compiler embeds canonical Core.Args source");
    assert!(source.source.contains("__args_count()"));
    assert!(source.source.contains("__args_get(i)"));

    let target = crate::abi_v5::TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target);
    let capability = canonical_corelib_service_capability(&manifest).expect("Corelib service capability");
    let args_services = capability
        .services()
        .iter()
        .filter(|service| service.source_path == CANONICAL_CORELIB_ARGS_SOURCE_PATH)
        .map(|service| (service.name, service.symbol))
        .collect::<Vec<_>>();

    assert_eq!(args_services, [("__args_count", "args_count"), ("__args_get", "args_get")]);
    assert!(capability.service_for_source(CANONICAL_CORELIB_ARGS_SOURCE_PATH, "__args_all").is_none());
}
