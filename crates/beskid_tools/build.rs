use std::path::{Path, PathBuf};

#[allow(dead_code)] // The build script uses the shared fingerprint implementation and embed filters.
#[path = "../beskid_abi/src/corelib_bundle.rs"]
mod corelib_fingerprint;
#[path = "../beskid_abi/corelib_workspace_source.rs"]
mod corelib_workspace_source;

use corelib_fingerprint::{
    CORELIB_BUNDLE_FINGERPRINT_FILE, copy_corelib_bundle, fingerprint_corelib_bundle_dir,
};

const ENV_CORELIB_SOURCE: &str = "BESKID_CORELIB_SOURCE";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../beskid_abi/corelib_workspace_source.rs");
    println!("cargo:rerun-if-changed=../beskid_abi/src/corelib_bundle.rs");

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR set by Cargo"));
    let corelib_workspace_dir = corelib_workspace_source::resolve_corelib_workspace(
        manifest_dir,
        std::env::var_os(ENV_CORELIB_SOURCE).as_deref(),
    )
    .unwrap_or_else(|| {
        panic!(
            "beskid_tools: corelib workspace not found. Expected `../../corelib` with a \
                 `.bws` workspace manifest plus `beskid_corelib/` (init the `compiler/corelib` \
                 submodule). Set {} to an absolute path to the **workspace** directory (parent of \
                 `beskid_corelib/`) to override. Hint: `git submodule update --init --recursive` \
                 from the compiler repo root.",
            ENV_CORELIB_SOURCE
        )
    });

    let dest = out_dir.join("embedded_corelib");
    if dest.exists() {
        std::fs::remove_dir_all(&dest).expect("remove stale embedded_corelib");
    }
    // The bundle is exactly the CoreLib.bws member inventory: the workspace manifest, root legal
    // files, and each member's package files under the package boundary rule. A missing or
    // malformed member fails the build instead of shipping a bundle that cannot resolve itself.
    let inventory = copy_corelib_bundle(&corelib_workspace_dir, &dest).unwrap_or_else(|error| {
        panic!(
            "beskid_tools: cannot derive the embedded Corelib bundle from {}: {error}",
            corelib_workspace_dir.display()
        )
    });
    let fingerprint = fingerprint_corelib_bundle_dir(&dest).expect("fingerprint embedded corelib");
    std::fs::write(
        dest.join(CORELIB_BUNDLE_FINGERPRINT_FILE),
        format!("{fingerprint}\n"),
    )
    .expect("write embedded corelib fingerprint");

    // Watch the workspace root (manifest and legal files, new members), every walked member
    // directory (added or removed sources), and every embedded file.
    println!("cargo:rerun-if-changed={}", corelib_workspace_dir.display());
    for relative in inventory.directories.iter().chain(&inventory.files) {
        println!(
            "cargo:rerun-if-changed={}",
            corelib_workspace_dir.join(relative).display()
        );
    }
    println!("cargo:rerun-if-env-changed={ENV_CORELIB_SOURCE}");
}
