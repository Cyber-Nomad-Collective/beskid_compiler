//! Corelib libraries without an `entry` that depend on `corelib_concurrency` must check their
//! own root module. Materialized dependency sources sort before the host root, so a dependency
//! unit (such as `Concurrency/Channel.bd`, whose `__channel_*` calls are Corelib services) must
//! never be selected as the entry program whose bodies the resolver checks under the host's
//! authority.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn corelib_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corelib").canonicalize().expect("compiler corelib checkout")
}

fn check_package(corelib: &Path, package: &str) -> (Output, String) {
    let project = corelib.join("packages").join(package);
    let output = Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
        .args(["check", "--plain", "--offline", "--project"])
        .arg(&project)
        .env("BESKID_CORELIB_ROOT", corelib)
        .current_dir(&project)
        .output()
        .expect("check dependent library");
    let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    (output, text)
}

#[test]
fn entryless_libraries_depending_on_concurrency_do_not_check_channel_as_their_entry() {
    let corelib = corelib_root();
    for package in ["console", "connect"] {
        let (_, text) = check_package(&corelib, package);
        assert!(!text.contains("__channel_"), "{package}: Channel service calls leaked into host analysis: {text}");
    }
    let (output, text) = check_package(&corelib, "connect");
    assert!(output.status.success(), "connect must check cleanly: {text}");
}

/// A Corelib package checked directly roots its own service files (`Concurrency/Channel.bd`,
/// network's DNS facade): the trust gate admits the package's own `obj/beskid/root` copy and the
/// resolver knows their `__*` service names.
#[test]
fn corelib_service_packages_check_their_own_service_files() {
    let corelib = corelib_root();
    for package in ["concurrency", "network"] {
        let (output, text) = check_package(&corelib, package);
        assert!(!text.contains("unknown value `__"), "{package}: service names unresolved in its own sources: {text}");
        assert!(output.status.success(), "{package} must check cleanly: {text}");
    }
}
