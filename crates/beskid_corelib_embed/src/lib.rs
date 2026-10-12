//! The Corelib bundle embedded in toolchain binaries.
//!
//! `build.rs` copies the `CoreLib.bws` member inventory of `../../corelib` (the workspace
//! manifest, root legal files, and every member's package files) into `OUT_DIR` and seals it with
//! its fingerprint marker. Only the binaries that provision a Corelib depend on this crate, so a
//! Corelib source edit rebuilds this leaf and relinks those binaries, not every crate that uses
//! `beskid_tools`.

pub use include_dir::Dir;

/// The sealed embedded Corelib bundle, passed to `beskid_tools::ensure_bundled_corelib`.
pub static CORELIB_BUNDLE: Dir<'static> = include_dir::include_dir!("$OUT_DIR/embedded_corelib");
