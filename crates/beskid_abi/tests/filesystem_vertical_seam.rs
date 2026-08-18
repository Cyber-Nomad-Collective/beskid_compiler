use std::{fs, path::PathBuf};

fn compiler_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn corelib_fs_source_exists() {
    let path = compiler_root().join("corelib/packages/foundation/src/Core/FS/FS.bd");
    assert!(path.exists(), "Core.FS source must exist at {}", path.display());
}

#[test]
fn runtime_fs_wrapper_source_exists() {
    let path = compiler_root().join("runtime/beskid/src/Runtime/Host/FS.bd");
    assert!(path.exists(), "runtime FS wrapper source must exist at {}", path.display());
}

#[test]
fn runtime_process_source_exists() {
    let path = compiler_root().join("runtime/beskid/src/Runtime/Host/Process.bd");
    assert!(path.exists(), "runtime Process source must exist at {}", path.display());
}

#[test]
fn runtime_syscalls_source_exists() {
    let path = compiler_root().join("runtime/beskid/src/Runtime/Io/Syscalls.bd");
    assert!(path.exists(), "runtime Syscalls source must exist at {}", path.display());
}

#[test]
fn runtime_scheduler_core_source_exists() {
    let path = compiler_root().join("runtime/beskid/src/Runtime/Fiber/Scheduler/Core.bd");
    assert!(path.exists(), "runtime Scheduler Core source must exist at {}", path.display());
}

#[test]
fn platform_host_c_sources_exist() {
    let root = compiler_root();
    for target in ["x86_64-unknown-linux-gnu", "aarch64-apple-darwin", "x86_64-pc-windows-msvc"] {
        let path = root.join("crates/beskid_abi/assembly").join(target).join("platform_host.c");
        assert!(path.exists(), "platform_host.c must exist for {target}");
    }
}

#[test]
fn runtime_manifest_exists() {
    let path = compiler_root().join("runtime_manifest.bsol");
    assert!(path.exists(), "runtime manifest must exist at {}", path.display());
}
