//! Manifest facts of the platform intrinsics. Each target's native kit build links the
//! platform host definitions, so a missing definition fails that build.

use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};

#[test]
fn canonical_unbound_intrinsics_have_exact_platform_definitions() {
    for target in TargetMetadata::supported() {
        let manifest = AbiManifestV5::canonical_runtime(target.clone());
        for name in ["memory_compare", "env_get", "env_set", "env_getcwd", "tty_winsize"] {
            let intrinsic = manifest
                .trusted_runtime_intrinsics
                .iter()
                .find(|intrinsic| intrinsic.name == name)
                .unwrap_or_else(|| panic!("canonical manifest declares {name}"));
            assert!(intrinsic.target_bindings.is_empty(), "{name} must use its one canonical ABI symbol");
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
    }
}
