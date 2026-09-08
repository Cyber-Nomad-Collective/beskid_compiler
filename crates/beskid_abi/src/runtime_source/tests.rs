use super::*;
use crate::abi_v5::AbiManifestV5;

#[test]
fn canonical_concurrency_facade_exposes_its_exact_scheduler_services() {
    let source = canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == CANONICAL_CORELIB_CONCURRENCY_SOURCE_PATH)
        .expect("compiler embeds canonical Concurrency facade");
    assert!(source.source.contains("__fiber_yield()"));
    assert!(source.source.contains("__fiber_now_millis()"));
    assert!(source.source.contains("__fiber_processor_count()"));

    let target = crate::abi_v5::TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target);
    let capability = canonical_corelib_service_capability(&manifest).expect("Corelib service capability");
    let services = capability
        .services()
        .iter()
        .filter(|service| service.source_path == CANONICAL_CORELIB_CONCURRENCY_SOURCE_PATH)
        .map(|service| (service.name, service.symbol))
        .collect::<Vec<_>>();

    assert_eq!(
        services,
        [
            ("__fiber_yield", "beskid_rt_v5_fiber_yield"),
            ("__fiber_now_millis", "fiber_now_millis"),
            ("__fiber_processor_count", "fiber_processor_count"),
        ]
    );
}

#[test]
fn canonical_fiber_facade_exposes_one_exact_join_and_cancel_contract() {
    const FIBER_FACADE: &str = "Concurrency/Fiber.bd";
    let source = canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == FIBER_FACADE)
        .expect("compiler embeds canonical Fiber facade");
    assert!(source.source.contains("__fiber_join_status(self.handle)"));
    assert!(!source.source.contains("__fiber_join(self.handle)"));
    assert!(source.source.contains("__fiber_cancel(self.handle, reason)"));

    let target = crate::abi_v5::TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target);
    let capability = canonical_corelib_service_capability(&manifest).expect("Corelib service capability");
    let services = capability
        .services()
        .iter()
        .filter(|service| service.source_path == FIBER_FACADE)
        .map(|service| (service.name, service.symbol))
        .collect::<Vec<_>>();

    assert_eq!(
        services,
        [
            ("__fiber_cancel", "fiber_cancel"),
            ("__fiber_detach", "fiber_detach"),
            ("__fiber_join_status", "fiber_join_status"),
            ("__fiber_join_value", "fiber_join_value"),
            ("__panic_str", "beskid_trap_message"),
        ]
    );
    let cancel =
        capability.service_for_source(FIBER_FACADE, "__fiber_cancel").expect("Fiber facade owns cancel authority");
    assert_eq!(
        canonical_corelib_service_abi(cancel),
        Some(CorelibServiceAbi {
            parameters: vec![CorelibServiceAbiType::I64, CorelibServiceAbiType::I64],
            result: CorelibServiceAbiType::U8,
        })
    );
    assert!(capability.service_for_source(FIBER_FACADE, "__fiber_join").is_none());
}

#[test]
fn canonical_channel_facade_uses_one_atomic_try_receive_claim_before_value_selection() {
    let source = canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == CANONICAL_CORELIB_CHANNEL_SOURCE_PATH)
        .expect("compiler embeds canonical Channel facade");
    assert!(source.source.contains("__channel_try_receive(self.handle)"));
    assert!(!source.source.contains("__channel_try_receive_status"));

    let target = crate::abi_v5::TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target);
    let capability = canonical_corelib_service_capability(&manifest).expect("Corelib service capability");
    let claim = capability
        .service_for_source(CANONICAL_CORELIB_CHANNEL_SOURCE_PATH, "__channel_try_receive")
        .expect("Channel facade owns exact atomic try-receive authority");
    assert_eq!(claim.symbol, "channel_try_receive");
    assert!(
        capability.service_for_source(CANONICAL_CORELIB_CHANNEL_SOURCE_PATH, "__channel_try_receive_status").is_none()
    );

    let value = capability
        .service_for_source(CANONICAL_CORELIB_CHANNEL_SOURCE_PATH, "__channel_receive_value")
        .expect("typed Channel value service");
    let dispatch = canonical_corelib_service_value_dispatch(value).expect("scalar/managed value dispatch");
    assert_eq!(dispatch.scalar_symbol, "channel_receive_value");
    assert_eq!(dispatch.managed_symbol, "channel_receive_ptr");
    assert_eq!(
        canonical_corelib_service_abi_for_adapter(dispatch.scalar_symbol),
        Some(CorelibServiceAbi { parameters: vec![CorelibServiceAbiType::I64], result: CorelibServiceAbiType::I64 })
    );
    assert_eq!(
        canonical_corelib_service_abi_for_adapter(dispatch.managed_symbol),
        Some(CorelibServiceAbi {
            parameters: vec![CorelibServiceAbiType::I64],
            result: CorelibServiceAbiType::Pointer,
        })
    );
}

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

    assert_eq!(args_services, [("__args_count", "beskid_rt_v5_args_count"), ("__args_get", "beskid_rt_v5_args_get")]);
    assert!(capability.service_for_source(CANONICAL_CORELIB_ARGS_SOURCE_PATH, "__args_all").is_none());
}

#[test]
fn canonical_console_linux_facade_alone_owns_terminal_size_service() {
    let source = canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == CANONICAL_CORELIB_CONSOLE_LINUX_SOURCE_PATH)
        .expect("compiler embeds canonical Console Linux facade");
    assert!(source.source.contains("__tty_winsize(fd)"));

    let target = crate::abi_v5::TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target);
    let capability = canonical_corelib_service_capability(&manifest).expect("Corelib service capability");
    let service = capability
        .service_for_source(CANONICAL_CORELIB_CONSOLE_LINUX_SOURCE_PATH, "__tty_winsize")
        .expect("Linux facade owns terminal-size service");
    assert_eq!(service.symbol, "tty_winsize");
    assert!(capability.service_for_source("Platform/Windows.bd", "__tty_winsize").is_none());
}

#[test]
fn canonical_array_source_exposes_only_the_array_len_service() {
    let target = crate::abi_v5::TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target);
    let capability = canonical_corelib_service_capability(&manifest).expect("Corelib source capability");

    assert!(capability.authorizes_source(CANONICAL_FOUNDATION_ARRAY_SOURCE_PATH));
    let array_services = capability
        .services()
        .iter()
        .filter(|service| service.source_path == CANONICAL_FOUNDATION_ARRAY_SOURCE_PATH)
        .map(|service| (service.name, service.symbol))
        .collect::<Vec<_>>();
    assert_eq!(array_services, [("__array_len", "array_len")]);
    assert!(
        capability.service_for_source(CANONICAL_FOUNDATION_ARRAY_SOURCE_PATH, "__array_new").is_none(),
        "typed allocation is compiler lowering, never a legacy size-only service import"
    );
}

#[test]
fn canonical_assert_source_can_force_collection_without_granting_other_units_gc_authority() {
    let target = crate::abi_v5::TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target);
    let capability = canonical_corelib_service_capability(&manifest).expect("Corelib service capability");

    let service = capability
        .service_for_source(CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH, "__gc_collect")
        .expect("Testing.Assert owns the forced-collection service");
    assert_eq!(service.symbol, "gc_collect");
    assert!(
        capability.service_for_source(CANONICAL_CORELIB_SYSCALL_SOURCE_PATH, "__gc_collect").is_none(),
        "unrelated Corelib units must not inherit forced-collection authority"
    );
}

#[test]
fn canonical_foundation_service_table_covers_every_implemented_raw_call_and_nothing_else() {
    let target = crate::abi_v5::TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target);
    let capability = canonical_corelib_service_capability(&manifest).expect("Corelib service capability");
    let sources = canonical_corelib_service_sources();

    for source in &sources {
        let raw_names = source
            .source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .flat_map(|line| line.split(|character: char| !(character.is_ascii_alphanumeric() || character == '_')))
            .filter(|token| token.starts_with("__"))
            .collect::<std::collections::BTreeSet<_>>();

        for name in &raw_names {
            let deliberately_compiler_lowered =
                source.logical_path == CANONICAL_FOUNDATION_ARRAY_SOURCE_PATH && *name == "__array_new";
            if deliberately_compiler_lowered {
                assert!(
                    capability.service_for_source(&source.logical_path, name).is_none(),
                    "{name} in {} must remain outside runtime service authority",
                    source.logical_path
                );
            } else {
                assert!(
                    capability.service_for_source(&source.logical_path, name).is_some(),
                    "implemented raw call {name} in {} needs an exact source-scoped service",
                    source.logical_path
                );
            }
        }

        for service in capability.services().iter().filter(|service| service.source_path == source.logical_path) {
            assert!(
                raw_names.contains(service.name),
                "service {} is granted to {} but the embedded source does not call it",
                service.name,
                source.logical_path
            );
        }
    }
}

#[test]
fn embedded_service_sources_and_service_descriptors_have_one_exact_path_inventory() {
    let target = crate::abi_v5::TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target);
    let capability = canonical_corelib_service_capability(&manifest).expect("Corelib service capability");
    let embedded = canonical_corelib_service_sources()
        .into_iter()
        .map(|source| source.logical_path)
        .collect::<std::collections::BTreeSet<_>>();
    let described = capability
        .services()
        .iter()
        .map(|service| service.source_path.to_owned())
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(embedded, described, "source bytes and service descriptors must not maintain divergent path lists");
    for logical_path in embedded {
        let physical = canonical_corelib_service_source_path(&logical_path)
            .unwrap_or_else(|| panic!("{logical_path} has no canonical physical path descriptor"));
        assert!(physical.is_file(), "{} must resolve to one canonical regular file", physical.display());
    }
}

#[test]
fn foundation_sources_do_not_directly_bind_raw_byte_buffer_services() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corelib/packages/foundation/src");
    let target = crate::abi_v5::TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target);
    let capability = canonical_corelib_service_capability(&manifest).expect("Corelib service capability");
    let mut pending = vec![root];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory).expect("read Foundation source directory") {
            let path = entry.expect("Foundation source entry").path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().and_then(std::ffi::OsStr::to_str) != Some("bd") {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("read Foundation source");
            let logical_path = path
                .strip_prefix(
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corelib/packages/foundation/src"),
                )
                .expect("Foundation source path")
                .to_string_lossy()
                .replace('\\', "/");
            let raw_names = source
                .lines()
                .filter(|line| !line.trim_start().starts_with("//"))
                .flat_map(|line| line.split(|character: char| !(character.is_ascii_alphanumeric() || character == '_')))
                .filter(|token| token.starts_with("__"))
                .collect::<std::collections::BTreeSet<_>>();
            for forbidden in ["__bytes_get", "__bytes_set", "__bytes_copy", "__bytes_compare"] {
                assert!(!source.contains(forbidden), "{} must not call raw-buffer service {forbidden}", path.display());
            }
            for name in raw_names {
                if name == "__array_new" {
                    assert!(
                        logical_path == CANONICAL_FOUNDATION_ARRAY_SOURCE_PATH && source.contains("__array_new<T>(0)"),
                        "{} must allocate arrays only through descriptor-backed compiler lowering",
                        path.display()
                    );
                    continue;
                }
                assert!(
                    capability.service_for_source(&logical_path, name).is_some(),
                    "raw Foundation call {name} in {logical_path} lacks exact source-scoped service authority"
                );
            }
        }
    }
}

#[test]
fn every_source_authorized_service_selects_one_generated_abi_shape() {
    let target = crate::abi_v5::TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target);
    let capability = canonical_corelib_service_capability(&manifest).expect("Corelib service capability");
    for service in capability.services() {
        canonical_corelib_service_abi(*service).unwrap_or_else(|| {
            panic!(
                "{} in {} must select one canonical generated-service or soft-builtin ABI shape",
                service.name, service.source_path
            )
        });
    }
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
