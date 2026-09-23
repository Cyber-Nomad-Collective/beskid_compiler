use super::*;
use crate::abi_v5::AbiManifestV5;

#[test]
fn canonical_bytes_copy_owns_only_the_numeric_bounds_trap_service() {
    for target in crate::abi_v5::TargetMetadata::supported() {
        let manifest = AbiManifestV5::canonical_runtime(target);
        let capability = canonical_corelib_service_capability(&manifest).unwrap();
        let services = capability
            .services()
            .iter()
            .filter(|service| service.source_path == "Core/Bytes/Slice.bd")
            .map(|service| (service.name, service.symbol))
            .collect::<Vec<_>>();
        assert_eq!(services, [("__panic", "beskid_trap_code")]);
        assert!(capability.service_for_source("app/Slice.bd", "__panic").is_none());
        let read =
            capability.service_for_source(CANONICAL_CORELIB_SYSCALL_SOURCE_PATH, "__syscall_read_bytes").unwrap();
        assert_eq!(
            canonical_corelib_service_abi(read),
            Some(CorelibServiceAbi {
                parameters: vec![
                    CorelibServiceAbiType::I32,
                    CorelibServiceAbiType::Pointer,
                    CorelibServiceAbiType::Usize
                ],
                result: CorelibServiceAbiType::I64,
            })
        );
    }
}

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
    assert!(source.source.contains("__fiber_join_status(handle)"));
    assert!(!source.source.contains("__fiber_join(handle)"));
    assert!(source.source.contains("__fiber_cancel(handle, 0_i64)"));

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
            ("__fiber_join_detail", "fiber_join_detail"),
            ("__fiber_join_message", "fiber_join_message"),
            ("__fiber_join_error_finish", "fiber_join_error_finish"),
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
    let dispatch = canonical_corelib_service_value_dispatch(value).expect("one traced value adapter");
    assert_eq!(dispatch.symbol, "channel_receive_value");
    assert_eq!(
        canonical_corelib_service_abi_for_adapter(dispatch.symbol),
        Some(CorelibServiceAbi {
            parameters: vec![CorelibServiceAbiType::I64, CorelibServiceAbiType::Pointer],
            result: CorelibServiceAbiType::U8
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
fn canonical_console_platform_facades_each_own_terminal_size_service() {
    let sources = canonical_corelib_service_sources();
    let platform_paths = [
        CANONICAL_CORELIB_CONSOLE_LINUX_SOURCE_PATH,
        CANONICAL_CORELIB_CONSOLE_MACOS_SOURCE_PATH,
        CANONICAL_CORELIB_CONSOLE_WINDOWS_SOURCE_PATH,
    ];
    for logical_path in platform_paths {
        let source = sources
            .iter()
            .find(|source| source.logical_path == logical_path)
            .unwrap_or_else(|| panic!("compiler embeds canonical Console platform facade {logical_path}"));
        assert!(source.source.contains("__tty_winsize(fd)"));
    }
    let terminal = sources
        .iter()
        .find(|source| source.logical_path == CANONICAL_CORELIB_CONSOLE_TERMINAL_SOURCE_PATH)
        .expect("compiler embeds canonical Console terminal facade");
    assert!(terminal.source.contains("__env_get(name)"));

    let target = crate::abi_v5::TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target);
    let capability = canonical_corelib_service_capability(&manifest).expect("Corelib service capability");
    for logical_path in platform_paths {
        let service = capability
            .service_for_source(logical_path, "__tty_winsize")
            .unwrap_or_else(|| panic!("{logical_path} owns terminal-size service"));
        assert_eq!(service.symbol, "tty_winsize");
    }
    let env_get = capability
        .service_for_source(CANONICAL_CORELIB_CONSOLE_TERMINAL_SOURCE_PATH, "__env_get")
        .expect("Terminal facade owns environment lookup service");
    assert_eq!(env_get.symbol, "env_get");
    assert!(capability.service_for_source("Copied/Platform/Terminal.bd", "__env_get").is_none());
    assert!(capability.service_for_source("Copied/Platform/MacOS.bd", "__tty_winsize").is_none());

    let canonical_hash = crate::abi_v5::canonical_source_hash(&sources).expect("canonical Corelib source hash");
    let mut altered = sources.clone();
    altered
        .iter_mut()
        .find(|source| source.logical_path == CANONICAL_CORELIB_CONSOLE_MACOS_SOURCE_PATH)
        .expect("embedded macOS facade")
        .source
        .push_str("\n// altered\n");
    assert_ne!(
        crate::abi_v5::canonical_source_hash(&altered).expect("altered Corelib source hash"),
        canonical_hash,
        "altered facade bytes must not match the compiler-owned source identity"
    );

    let mut copied = sources;
    copied
        .iter_mut()
        .find(|source| source.logical_path == CANONICAL_CORELIB_CONSOLE_WINDOWS_SOURCE_PATH)
        .expect("embedded Windows facade")
        .logical_path = "Copied/Platform/Windows.bd".into();
    assert_ne!(
        crate::abi_v5::canonical_source_hash(&copied).expect("copied Corelib source hash"),
        canonical_hash,
        "copied facade paths must not match the compiler-owned source identity"
    );
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
fn canonical_time_source_alone_owns_the_manifest_sleep_binding() {
    for target in crate::abi_v5::TargetMetadata::supported() {
        let manifest = AbiManifestV5::canonical_runtime(target);
        let capability = canonical_corelib_service_capability(&manifest).expect("Corelib service capability");
        let service = capability
            .service_for_source(CANONICAL_FOUNDATION_TIME_SOURCE_PATH, "__timer_sleep_until")
            .expect("Core.Time owns the scheduler sleep binding");
        assert_eq!(service.symbol, "beskid_rt_v5_external_sleep_until");
        assert_eq!(
            preflight_corelib_service_import(&capability, &manifest, service).expect("manifest source builtin"),
            CorelibServiceAbi { parameters: vec![CorelibServiceAbiType::I64], result: CorelibServiceAbiType::Usize },
            "the sleep binding must keep the export's word-sized status result"
        );
        assert!(
            capability.services().iter().filter(|candidate| candidate.name == "__timer_sleep_until").count() == 1,
            "no other Corelib unit may acquire scheduler sleep authority"
        );
        assert!(capability.service_for_source("app/Core/Time/Time.bd", "__timer_sleep_until").is_none());
        assert!(capability.service_for_source(CANONICAL_CORELIB_FIBER_SOURCE_PATH, "__timer_sleep_until").is_none());
    }
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
        let identity = corelib_service_source_identity(&logical_path).expect("complete source identity");
        let physical = canonical_corelib_service_source_path(&logical_path)
            .unwrap_or_else(|| panic!("{logical_path} has no canonical physical path descriptor"));
        assert!(physical.is_file(), "{} must resolve to one canonical regular file", physical.display());
        assert_eq!(identity.canonical_path, physical);
        assert_eq!(identity.declared_path.canonicalize().unwrap(), physical);
        assert!(!identity.declared_path.components().any(|part| matches!(part, std::path::Component::ParentDir)));
    }
    assert!(corelib_service_source_identity("user/Core/Syscall/Syscall.bd").is_none());
}

#[cfg(windows)]
#[test]
fn source_location_comparison_only_equates_drive_prefix_representations() {
    use std::path::Path;
    let normal = Path::new(r"C:\compiler\corelib\Syscall.bd");
    assert!(corelib_source_locations_match(normal, Path::new(r"\\?\C:\compiler\corelib\Syscall.bd")));
    for different in [
        r"D:\compiler\corelib\Syscall.bd",
        r"C:\user\Syscall.bd",
        r"C:\compiler\alias\..\corelib\Syscall.bd",
        r"\\.\C:\compiler\corelib\Syscall.bd",
        r"\\?\UNC\compiler\corelib\Syscall.bd",
    ] {
        assert!(!corelib_source_locations_match(normal, Path::new(different)), "{different}");
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

#[test]
fn every_source_authorized_service_passes_the_exact_target_import_preflight() {
    for target in crate::abi_v5::TargetMetadata::supported() {
        let manifest = AbiManifestV5::canonical_runtime(target);
        let capability = canonical_corelib_service_capability(&manifest).expect("Corelib service capability");
        for service in capability.services() {
            let preflight =
                preflight_corelib_service_import(&capability, &manifest, *service).unwrap_or_else(|error| {
                    panic!("{} in {} failed import preflight: {error}", service.name, service.source_path)
                });
            assert_eq!(preflight, canonical_corelib_service_abi(*service).expect("canonical service ABI"));
        }
    }
}

#[test]
fn corelib_import_preflight_rejects_unknown_source_and_mismatched_adapter_declarations() {
    let target = crate::abi_v5::TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target);
    let capability = canonical_corelib_service_capability(&manifest).expect("Corelib service capability");
    let service = capability
        .service_for_source(CANONICAL_CORELIB_SYSCALL_SOURCE_PATH, "__syscall_read_bytes")
        .expect("canonical syscall service");

    assert!(matches!(
        preflight_corelib_service_declaration(
            &capability,
            &manifest,
            "Copied/Core/Syscall/Syscall.bd",
            service.name,
            service.symbol,
        ),
        Err(CorelibServiceImportPreflightError::UnauthorizedDeclaration { .. })
    ));
    assert!(matches!(
        preflight_corelib_service_declaration(
            &capability,
            &manifest,
            service.source_path,
            service.name,
            "copied_runtime_adapter",
        ),
        Err(CorelibServiceImportPreflightError::UnauthorizedDeclaration { .. })
    ));

    assert!(capability.service_for_source(CANONICAL_CORELIB_CHANNEL_SOURCE_PATH, "__channel_receive").is_none());
    assert!(capability.service_for_source(CANONICAL_CORELIB_CHANNEL_SOURCE_PATH, "__channel_receive_status").is_some());
}

#[test]
fn networking_imports_are_source_scoped_and_target_shape_exact() {
    const NETWORK_SERVICES: &[(&str, &str)] = &[
        ("__network_open", "beskid_rt_v5_network_open"),
        ("__network_accept", "beskid_rt_v5_network_accept"),
        ("__network_close", "beskid_rt_v5_network_close"),
        ("__network_read", "beskid_rt_v5_network_read"),
        ("__network_write", "beskid_rt_v5_network_write"),
        ("__network_address", "beskid_rt_v5_network_address"),
        ("__network_options", "beskid_rt_v5_network_options"),
        ("__network_set_options", "beskid_rt_v5_network_set_options"),
        ("__network_shutdown_write", "beskid_rt_v5_network_shutdown_write"),
        ("__network_udp_connect", "beskid_rt_v5_network_udp_connect"),
        ("__network_receive", "beskid_rt_v5_network_receive"),
        ("__network_send", "beskid_rt_v5_network_send"),
        ("__network_dns_resolve", "beskid_rt_v5_network_dns_resolve"),
        ("__network_dns_count", "beskid_rt_v5_network_dns_count"),
        ("__network_dns_address", "beskid_rt_v5_network_dns_address"),
        ("__network_dns_release", "beskid_rt_v5_network_dns_release"),
    ];

    for target in crate::abi_v5::TargetMetadata::supported() {
        let manifest = AbiManifestV5::canonical_runtime(target);
        let capability = canonical_corelib_service_capability(&manifest).expect("Corelib service capability");
        for (name, symbol) in NETWORK_SERVICES {
            let abi = preflight_corelib_service_declaration(
                &capability,
                &manifest,
                CANONICAL_NETWORK_INTERNAL_SOURCE_PATH,
                name,
                symbol,
            )
            .unwrap_or_else(|error| panic!("network declaration {name} / {symbol} failed preflight: {error}"));
            assert!(
                !abi.parameters.is_empty() || !matches!(abi.result, CorelibServiceAbiType::Void),
                "network service {name} must retain a concrete target shape"
            );
        }
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

/// The growable-heap design (`docs/superpowers/specs/2026-09-22-growable-gc-heap-design.md`)
/// fixes one canonical offset table shared by `runtime_manifest.bsol` (`BeskidHeapState`,
/// `BeskidHeapRegion`) and the hand-written `HEAP_*`/`REGION_*` constants that every `.bd` GC
/// module re-declares. Nothing before this test caught the two from drifting apart; this parses
/// every `HEAP_*` and `REGION_*` numeric constant declared across the canonical GC sources and
/// asserts each one that names a manifest offset or a layout size agrees with the manifest.
#[test]
fn heap_source_constants_match_the_manifest_heap_layouts() {
    use crate::abi_v5::AbiManifestV5;
    use std::collections::BTreeMap;

    let heap_source_paths = [
        CANONICAL_GC_STATE_SOURCE_PATH,
        CANONICAL_GC_ALLOCATION_SOURCE_PATH,
        CANONICAL_GC_COLLECTION_SOURCE_PATH,
        CANONICAL_GC_SWEEP_SOURCE_PATH,
        CANONICAL_GC_ROOTS_HANDLES_SOURCE_PATH,
        CANONICAL_BOOTSTRAP_LIFECYCLE_SOURCE_PATH,
    ];
    let all_sources = canonical_runtime_sources();
    let sources = heap_source_paths.into_iter().map(|logical_path| {
        let unit = all_sources
            .iter()
            .find(|unit| unit.logical_path == logical_path)
            .unwrap_or_else(|| panic!("compiler embeds canonical heap source {logical_path}"));
        (logical_path, unit.source.as_str())
    });

    // name -> (value, first source path that declared it)
    let mut declared: BTreeMap<String, (i64, &str)> = BTreeMap::new();
    for (path, source) in sources {
        for line in source.lines() {
            let trimmed = line.trim();
            let Some(rest) = trimmed.strip_prefix("const ") else { continue };
            let Some((name, rest)) = rest.split_once('=') else { continue };
            let name = name.trim();
            if !(name.starts_with("HEAP_") || name.starts_with("REGION_")) {
                continue;
            }
            let Some(value_text) = rest.trim().strip_suffix(';') else { continue };
            let Ok(value) = value_text.trim().parse::<i64>() else { continue };
            match declared.get(name) {
                None => {
                    declared.insert(name.to_string(), (value, path));
                }
                Some((existing, first_path)) => assert_eq!(
                    *existing, value,
                    "`{name}` disagrees between `{first_path}` ({existing}) and `{path}` ({value}); every canonical \
                     GC source must declare the same heap-layout constants"
                ),
            }
        }
    }

    let manifest = AbiManifestV5::canonical_runtime(crate::abi_v5::TargetMetadata::supported()[0].clone());
    let heap_state = manifest.layouts.iter().find(|layout| layout.name == "BeskidHeapState").unwrap();
    let heap_region = manifest.layouts.iter().find(|layout| layout.name == "BeskidHeapRegion").unwrap();

    // Constant name -> expected value, derived from the manifest layouts.
    let mut expected: BTreeMap<String, i64> = BTreeMap::new();
    expected.insert("HEAP_STATE_SIZE".into(), heap_state.size as i64);
    for field in &heap_state.fields {
        expected.insert(format!("HEAP_{}", field.name.to_uppercase()), field.offset as i64);
    }
    expected.insert("REGION_HEADER_SIZE".into(), heap_region.size as i64);
    for field in &heap_region.fields {
        expected.insert(format!("REGION_{}", field.name.to_uppercase()), field.offset as i64);
    }

    for (name, expected_value) in &expected {
        let (actual_value, path) = declared
            .get(name)
            .unwrap_or_else(|| panic!("no canonical GC source declares `{name}` (expected {expected_value})"));
        assert_eq!(
            actual_value, expected_value,
            "`{name}` in `{path}` is {actual_value} but the manifest `BeskidHeapState`/`BeskidHeapRegion` layout \
             says {expected_value}"
        );
    }
}
