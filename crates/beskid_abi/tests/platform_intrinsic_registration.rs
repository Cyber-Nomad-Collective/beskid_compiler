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
