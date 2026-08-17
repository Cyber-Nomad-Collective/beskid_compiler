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

/// A canonical `CorelibService` round-trips through serde and recovers the same `&'static str`
/// triple from the compile-time table, while an unknown triple fails closed.
#[test]
fn corelib_service_serde_round_trips_canonical_and_rejects_unknown() {
    let target = crate::abi_v5::TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target);
    let capability = canonical_corelib_service_capability(&manifest).expect("Corelib service capability");
    let canonical = capability.services().first().copied().expect("at least one Corelib service");
    let json = serde_json::to_string(&canonical).expect("serialize CorelibService");
    let recovered: CorelibService = serde_json::from_str(&json).expect("deserialize CorelibService");
    // The recovered entry must equal the canonical table entry. (`&str` equality is byte-wise, so
    // `const`-inlining pointer differences between use sites of `CORELIB_SERVICES` do not matter;
    // salsa persistence compares `&str` by `Eq`/`Hash`, not by pointer.)
    assert_eq!(recovered, canonical);

    // An unknown triple must fail closed (no silent non-canonical recovery).
    let tampered = serde_json::json!({
        "name": "__not_a_service",
        "symbol": "not_a_symbol",
        "source_path": "not/a/source/path",
    });
    let err = serde_json::from_value::<CorelibService>(tampered).expect_err("unknown CorelibService fails closed");
    assert!(err.to_string().contains("unknown CorelibService"));
}

/// The `recover_static_str` helper recovers a canonical `&'static str` and fails closed on an
/// unknown value with the caller-supplied `what` label in the error message.
#[test]
fn recover_static_str_round_trips_canonical_and_fails_closed() {
    use crate::serde_support::recover_static_str;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct Wrapper(#[serde(deserialize_with = "deserialize_symbol")] String);

    fn deserialize_symbol<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
        let canonical = recover_static_str(deserializer, "test symbol", |value| {
            ["alpha", "beta", "gamma"].iter().copied().find(|candidate| *candidate == value)
        })?;
        Ok(canonical.to_owned())
    }

    let wrapper: Wrapper = serde_json::from_str("\"beta\"").expect("known symbol deserializes");
    assert_eq!(wrapper.0, "beta");

    let err = serde_json::from_str::<Wrapper>("\"delta\"").expect_err("unknown symbol fails closed");
    assert!(err.to_string().contains("unknown test symbol `delta`"));
}
