//! Embedded native ABI-v5 assembly/C sources for `beskid_aot`'s platform-object linking.
//!
//! `beskid_aot::api::platform_objects` used to resolve these sources with
//! `PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../beskid_abi/assembly")` and read them
//! straight off disk. That path is baked into the compiled binary at *its own* build time,
//! so a released `beskid` binary only worked when the exact source checkout it was built
//! from (down to the absolute path) was still present at runtime — true on a fresh CI
//! runner, never true for a binary downloaded from a release and run anywhere else.
//!
//! Every other native/runtime source this crate needs (the corelib and Beskid runtime
//! bootstrap sources in `runtime_source`) is already embedded into the binary at compile
//! time via `include_str!` for exactly this reason. This module does the same for the
//! handful of assembly/C files clang, llvm-ml, and cl need to see as real files on disk,
//! then [`stage_into`] writes them into a caller-supplied directory just before compiling,
//! preserving their original relative layout so their own `#include`s keep resolving.

use std::io;
use std::path::{Path, PathBuf};

struct EmbeddedFile {
    /// Path relative to the staging root, matching this crate's own `assembly/`/`include/`
    /// layout so relative `#include`s between staged files resolve unchanged.
    relative_path: &'static str,
    contents: &'static str,
}

const FILES: &[EmbeddedFile] = &[
    EmbeddedFile {
        relative_path: "include/beskid_runtime_abi_v5.h",
        contents: include_str!("../include/beskid_runtime_abi_v5.h"),
    },
    EmbeddedFile {
        relative_path: "assembly/common/args_utf16.h",
        contents: include_str!("../assembly/common/args_utf16.h"),
    },
    EmbeddedFile {
        relative_path: "assembly/common/network.h",
        contents: include_str!("../assembly/common/network.h"),
    },
    EmbeddedFile {
        relative_path: "assembly/common/network_posix.h",
        contents: include_str!("../assembly/common/network_posix.h"),
    },
    EmbeddedFile {
        relative_path: "assembly/common/external_wait.h",
        contents: include_str!("../assembly/common/external_wait.h"),
    },
    EmbeddedFile {
        relative_path: "assembly/common/executable_bootstrap.c",
        contents: include_str!("../assembly/common/executable_bootstrap.c"),
    },
    EmbeddedFile {
        relative_path: "assembly/x86_64-unknown-linux-gnu/context.S",
        contents: include_str!("../assembly/x86_64-unknown-linux-gnu/context.S"),
    },
    EmbeddedFile {
        relative_path: "assembly/x86_64-unknown-linux-gnu/platform.S",
        contents: include_str!("../assembly/x86_64-unknown-linux-gnu/platform.S"),
    },
    EmbeddedFile {
        relative_path: "assembly/x86_64-unknown-linux-gnu/platform_tls.c",
        contents: include_str!("../assembly/x86_64-unknown-linux-gnu/platform_tls.c"),
    },
    EmbeddedFile {
        relative_path: "assembly/x86_64-unknown-linux-gnu/platform_host.c",
        contents: include_str!("../assembly/x86_64-unknown-linux-gnu/platform_host.c"),
    },
    EmbeddedFile {
        relative_path: "assembly/aarch64-apple-darwin/context.S",
        contents: include_str!("../assembly/aarch64-apple-darwin/context.S"),
    },
    EmbeddedFile {
        relative_path: "assembly/aarch64-apple-darwin/platform.S",
        contents: include_str!("../assembly/aarch64-apple-darwin/platform.S"),
    },
    EmbeddedFile {
        relative_path: "assembly/aarch64-apple-darwin/platform_tls.c",
        contents: include_str!("../assembly/aarch64-apple-darwin/platform_tls.c"),
    },
    EmbeddedFile {
        relative_path: "assembly/aarch64-apple-darwin/platform_host.c",
        contents: include_str!("../assembly/aarch64-apple-darwin/platform_host.c"),
    },
    EmbeddedFile {
        relative_path: "assembly/x86_64-pc-windows-msvc/context.asm",
        contents: include_str!("../assembly/x86_64-pc-windows-msvc/context.asm"),
    },
    EmbeddedFile {
        relative_path: "assembly/x86_64-pc-windows-msvc/platform.asm",
        contents: include_str!("../assembly/x86_64-pc-windows-msvc/platform.asm"),
    },
    EmbeddedFile {
        relative_path: "assembly/x86_64-pc-windows-msvc/platform_tls.c",
        contents: include_str!("../assembly/x86_64-pc-windows-msvc/platform_tls.c"),
    },
    EmbeddedFile {
        relative_path: "assembly/x86_64-pc-windows-msvc/platform_host.c",
        contents: include_str!("../assembly/x86_64-pc-windows-msvc/platform_host.c"),
    },
    EmbeddedFile {
        relative_path: "assembly/x86_64-pc-windows-msvc/network_iocp.h",
        contents: include_str!("../assembly/x86_64-pc-windows-msvc/network_iocp.h"),
    },
];

/// Writes every embedded native ABI source into `dest_root`, recreating the
/// `assembly/<target-triple>/...`, `assembly/common/...`, and `include/...` layout the
/// sources have in this crate, so their relative `#include`s keep resolving unchanged.
/// Returns `dest_root/assembly`, the drop-in replacement for the old
/// `CARGO_MANIFEST_DIR`-derived assembly root.
///
/// Idempotent and cheap (a couple dozen small text files): callers may stage into the
/// same `output_dir` on every native-object compile without needing to cache the result.
pub fn stage_into(dest_root: &Path) -> io::Result<PathBuf> {
    for file in FILES {
        let dest = dest_root.join(file.relative_path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, file.contents)?;
    }
    Ok(dest_root.join("assembly"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_every_embedded_file_and_preserves_relative_includes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let assembly_root = stage_into(temp.path()).expect("stage_into");
        assert_eq!(assembly_root, temp.path().join("assembly"));

        // Spot-check the two-level-up include relationship platform_host.c relies on:
        // assembly/<target>/platform_host.c -> ../../include/beskid_runtime_abi_v5.h.
        let header = assembly_root.join("x86_64-unknown-linux-gnu").join("../../include/beskid_runtime_abi_v5.h");
        assert!(header.exists(), "staged tree must satisfy platform_host.c's ../../include/ path: {header:?}");

        // Every embedded file actually landed.
        for file in FILES {
            let dest = temp.path().join(file.relative_path);
            assert!(dest.is_file(), "expected staged file at {dest:?}");
            assert_eq!(std::fs::read_to_string(&dest).unwrap(), file.contents);
        }
    }
}
