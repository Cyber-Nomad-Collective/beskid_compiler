use std::fs;
use std::path::PathBuf;

use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};

fn compiler_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn canonical_unbound_intrinsics_have_exact_platform_definitions() {
    for target in TargetMetadata::supported() {
        let manifest = AbiManifestV5::canonical_runtime(target.clone());
        let platform_source = fs::read_to_string(
            compiler_root().join("crates/beskid_abi/assembly").join(target.triple.as_str()).join("platform_host.c"),
        )
        .expect("platform host source");

        for name in ["memory_compare", "env_get", "env_set", "env_getcwd", "tty_winsize"] {
            let intrinsic = manifest
                .trusted_runtime_intrinsics
                .iter()
                .find(|intrinsic| intrinsic.name == name)
                .unwrap_or_else(|| panic!("canonical manifest declares {name}"));
            assert!(intrinsic.target_bindings.is_empty(), "{name} must use its one canonical ABI symbol");
            assert!(
                platform_source.contains(&format!("{}(", intrinsic.symbol)),
                "{} must define unbound intrinsic `{}`",
                target.triple.as_str(),
                intrinsic.symbol
            );
        }
    }
}

#[test]
fn canonical_thread_yield_uses_a_host_specific_os_primitive() {
    let expected = [
        ("x86_64-unknown-linux-gnu", "sched_yield"),
        ("aarch64-apple-darwin", "sched_yield"),
        ("x86_64-pc-windows-msvc", "SwitchToThread"),
    ];
    for (triple, os_import) in expected {
        let target = TargetMetadata::supported()
            .into_iter()
            .find(|target| target.triple.as_str() == triple)
            .unwrap_or_else(|| panic!("supported target {triple}"));
        let manifest = AbiManifestV5::canonical_runtime(target);
        let intrinsic = manifest
            .trusted_runtime_intrinsics
            .iter()
            .find(|intrinsic| intrinsic.name == "thread_yield")
            .expect("canonical manifest declares thread_yield");
        let binding = intrinsic
            .target_bindings
            .iter()
            .find(|binding| binding.target == triple)
            .expect("host-specific thread_yield binding");
        assert_eq!(binding.os_imports, [os_import]);
        let platform_source = fs::read_to_string(
            compiler_root().join("crates/beskid_abi/assembly").join(triple).join("platform_host.c"),
        )
        .expect("platform host source");
        assert!(
            platform_source.contains("beskid_rt_v5_thread_yield("),
            "{triple} must define its OS thread yield adapter"
        );
        assert!(platform_source.contains(os_import), "{triple} adapter must call {os_import}");
    }
}

#[test]
fn canonical_trap_export_delegates_to_a_distinct_platform_intrinsic() {
    for target in TargetMetadata::supported() {
        let manifest = AbiManifestV5::canonical_runtime(target.clone());
        let trap = manifest
            .trusted_runtime_intrinsics
            .iter()
            .find(|intrinsic| intrinsic.name == "trap")
            .expect("canonical manifest declares trap");
        assert_eq!(trap.symbol, "beskid_rt_v5_intrinsic_trap");
        assert_ne!(trap.symbol, "beskid_rt_v5_trap", "the source-owned export must not call itself");

        let platform_source = fs::read_to_string(
            compiler_root().join("crates/beskid_abi/assembly").join(target.triple.as_str()).join("platform_host.c"),
        )
        .expect("platform host source");
        assert!(
            platform_source.contains("beskid_rt_v5_intrinsic_trap("),
            "{} must define the terminal trap primitive",
            target.triple.as_str()
        );
    }
}
