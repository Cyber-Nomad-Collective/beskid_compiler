use std::path::{Path, PathBuf};

pub(super) fn corelib_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    for root in corelib_source_candidates(manifest_dir) {
        if root.join("corelib.bproj").is_file() {
            return root;
        }
    }
    panic!(
        "canonical corelib root not found. expected compiler/corelib/beskid_corelib (init the corelib submodule); looked near {}",
        manifest_dir.display()
    );
}

/// Parent `compiler/corelib` directory containing `Workspace.proj` and `packages/`.
pub(super) fn corelib_workspace_root() -> PathBuf {
    let pkg = corelib_root();
    pkg.parent()
        .expect("corelib package must live under compiler/corelib workspace root")
        .to_path_buf()
}

pub(super) fn foundation_src() -> PathBuf {
    corelib_workspace_root().join("packages/foundation/src")
}

pub(super) fn runtime_src() -> PathBuf {
    corelib_workspace_root().join("packages/runtime/src")
}

pub(super) fn compiler_sdk_src() -> PathBuf {
    corelib_workspace_root().join("packages/compiler-sdk/src")
}

fn corelib_source_candidates(manifest_dir: &Path) -> [PathBuf; 1] {
    [manifest_dir.join("../../corelib/beskid_corelib")]
}

/// Representative corelib sources for fast parse smoke (full inventory stays in `layout`).
pub(super) fn stratified_corelib_parse_samples() -> &'static [&'static str] {
    &[
        "packages/foundation/src/Prelude.bd",
        "packages/runtime/src/System/Syscall.bd",
        "packages/console/src/Platform/Terminal.bd",
    ]
}

/// Foundation, runtime, and console packages analyzed in project context (excludes compiler-sdk).
pub(super) fn stratified_corelib_analyze_sources() -> impl Iterator<Item = &'static str> {
    expected_corelib_workspace_sources()
        .iter()
        .copied()
        .filter(|path| !path.contains("compiler-sdk"))
}

/// Workspace-relative `.bd` sources that must exist for the split corelib layout.
pub(super) fn expected_corelib_workspace_sources() -> &'static [&'static str] {
    &[
        "packages/foundation/src/Prelude.bd",
        "packages/foundation/src/Collections/Array.bd",
        "packages/foundation/src/Collections/List.bd",
        "packages/foundation/src/Collections/Map.bd",
        "packages/foundation/src/Collections/Queue.bd",
        "packages/foundation/src/Collections/Set.bd",
        "packages/foundation/src/Collections/Stack.bd",
        "packages/foundation/src/Core/ErrorHandling.bd",
        "packages/foundation/src/Core/Results.bd",
        "packages/foundation/src/Core/String.bd",
        "packages/foundation/src/Query/Contracts.bd",
        "packages/foundation/src/Query/Execution.bd",
        "packages/foundation/src/Query/Operators.bd",
        "packages/foundation/src/Testing/Assertions.bd",
        "packages/foundation/src/Testing/Contracts.bd",
        "packages/runtime/src/Prelude.bd",
        "packages/runtime/src/System/Environment.bd",
        "packages/runtime/src/System/FS.bd",
        "packages/runtime/src/System/Input.bd",
        "packages/runtime/src/System/Output.bd",
        "packages/runtime/src/System/Error.bd",
        "packages/console/src/Prelude.bd",
        "packages/console/src/Console.bd",
        "packages/console/src/Ansi/Escape.bd",
        "packages/console/src/Ansi/Contracts.bd",
        "packages/console/src/Ansi/StyleChain.bd",
        "packages/console/src/Ansi/Sgr.bd",
        "packages/console/src/Ansi/Cursor.bd",
        "packages/console/src/Ansi/Erase.bd",
        "packages/console/src/Ansi/Screen.bd",
        "packages/console/src/Ansi/Osc.bd",
        "packages/console/src/Ansi/InputMode.bd",
        "packages/console/src/Console/Capabilities.bd",
        "packages/console/src/Console/Format.bd",
        "packages/console/src/Console/Format/Scan.bd",
        "packages/console/src/Console/Format/Attributes.bd",
        "packages/console/src/Console/Format/Markdown.bd",
        "packages/console/src/Platform/Terminal.bd",
        "packages/runtime/src/System/Path.bd",
        "packages/runtime/src/System/Process.bd",
        "packages/runtime/src/System/Syscall.bd",
        "packages/runtime/src/System/Syscall/Descriptor.bd",
        "packages/runtime/src/System/Syscall/ReadRequest.bd",
        "packages/runtime/src/System/Syscall/ReadLimit.bd",
        "packages/runtime/src/System/Syscall/StandardStream.bd",
        "packages/runtime/src/System/Syscall/SyscallError.bd",
        "packages/runtime/src/System/Syscall/WriteRequest.bd",
        "packages/runtime/src/System/Time.bd",
        "packages/runtime/src/Runtime/Abi.bd",
        "packages/runtime/src/Runtime/Init.bd",
        "packages/compiler-sdk/src/Prelude.bd",
        "packages/compiler-sdk/src/Beskid/Syntax.bd",
        "packages/compiler-sdk/src/Beskid/Syntax/Nodes.bd",
        "packages/compiler-sdk/src/Beskid/Compiler/TypedEmitter.bd",
        "packages/compiler-sdk/src/Beskid/Compiler/Collect.bd",
        "packages/compiler-sdk/src/Beskid/Compiler/Compilation.bd",
    ]
}

mod compile;
mod layout;
