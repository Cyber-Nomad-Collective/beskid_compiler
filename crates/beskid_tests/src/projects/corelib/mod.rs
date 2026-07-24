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
    pkg.parent().expect("corelib package must live under compiler/corelib workspace root").to_path_buf()
}

pub(super) fn foundation_src() -> PathBuf {
    corelib_workspace_root().join("packages/foundation/src")
}

pub(super) fn foundation_root() -> PathBuf {
    corelib_workspace_root().join("packages/foundation")
}

pub(super) fn compiler_sdk_src() -> PathBuf {
    corelib_workspace_root().join("packages/compiler-sdk/src")
}

fn corelib_source_candidates(manifest_dir: &Path) -> [PathBuf; 1] {
    [manifest_dir.join("../../corelib/beskid_corelib")]
}

/// Representative corelib sources for fast parse smoke (full inventory stays in `layout`).
pub(super) fn stratified_corelib_parse_samples() -> &'static [&'static str] {
    &["packages/foundation/src/Core/Results/Results.bd", "packages/console/src/Platform/Terminal.bd"]
}

/// Workspace-relative `.bd` sources that must exist for the split corelib layout.
pub(super) fn expected_corelib_workspace_sources() -> &'static [&'static str] {
    &[
        "packages/foundation/src/Collections/Collections.bd",
        "packages/foundation/src/Collections/Array.bd",
        "packages/foundation/src/Collections/Array/ArrayIter.bd",
        "packages/foundation/src/Collections/List.bd",
        "packages/foundation/src/Collections/Map.bd",
        "packages/foundation/src/Collections/Map/MapEntry.bd",
        "packages/foundation/src/Collections/Queue.bd",
        "packages/foundation/src/Collections/Set.bd",
        "packages/foundation/src/Collections/Stack.bd",
        "packages/foundation/src/Core/ErrorHandling/ErrorHandling.bd",
        "packages/foundation/src/Core/Fluent/Step.bd",
        "packages/compiler-sdk/src/Beskid/Fluent.bd",
        "packages/foundation/src/Core/Optional/Option.bd",
        "packages/foundation/src/Core/Results/Results.bd",
        "packages/foundation/src/Core/String/String.bd",
        "packages/foundation/src/Core/Text/Cursor.bd",
        "packages/foundation/src/Core/Text/Parser.bd",
        "packages/foundation/src/Core/Text/Parser/Literals.bd",
        "packages/foundation/src/Core/Text/Parser/Combine.bd",
        "packages/foundation/src/Core/Text/Parser/Cardinality.bd",
        "packages/foundation/src/Core/Text/Parser/Flow.bd",
        "packages/foundation/src/Core/Text/Parser/Context.bd",
        "packages/foundation/src/Core/Text/Parser/Terms.bd",
        "packages/foundation/src/Core/Text/Parser/Coordination.bd",
        "packages/foundation/src/Core/Text/Pest.bd",
        "packages/foundation/src/Core/Text/Casing.bd",
        "packages/foundation/src/Core/Text/Pest/Expr.bd",
        "packages/foundation/src/Core/Text/Pest/Names.bd",
        "packages/foundation/src/Core/Text/Pest/Grammar.bd",
        "packages/foundation/src/Core/Text/Pest/Emit.bd",
        "packages/foundation/src/Core/Text/Regex.bd",
        "packages/foundation/.generated/Core/Text/Regex/Generated.g.bd",
        "packages/pest-gen-schema/schemas/pestGenConfig.v1.bsol",
        "packages/pest-gen-schema/pest_gen_schema.bproj",
        "mods/corelib_pest_gen/corelib_pest_gen.bproj",
        "mods/corelib_pest_gen/Src/Mod.bd",
        "mods/corelib_pest_gen/Src/Targets.bd",
        "mods/corelib_pest_gen/Src/Emit.bd",
        "mods/corelib_pest_gen/generate.layout.json",
        "packages/foundation/src/Query/Execution.bd",
        "packages/foundation/src/Query/Operators.bd",
        "packages/foundation/src/Query/Query.bd",
        "packages/foundation/src/Query/QueryState.bd",
        "packages/foundation/src/Testing/Assert.bd",
        "packages/foundation/src/Testing/Contracts.bd",
        "packages/foundation/src/Testing/Testing.bd",
        "packages/foundation/src/Core/Environment/Environment.bd",
        "packages/foundation/src/Core/Environment/EnvironmentError.bd",
        "packages/foundation/src/Core/FS/FS.bd",
        "packages/foundation/src/Core/FS/FsError.bd",
        "packages/foundation/src/Core/Input/Input.bd",
        "packages/foundation/src/Core/Output/Output.bd",
        "packages/foundation/src/Core/Error/Error.bd",
        "packages/foundation/src/Core/Threading/Thread.bd",
        "packages/foundation/src/Core/Threading/ThreadError.bd",
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
        "packages/foundation/src/Core/Path/Path.bd",
        "packages/foundation/src/Core/Process/Process.bd",
        "packages/foundation/src/Core/Process/ProcessError.bd",
        "packages/foundation/src/Core/Syscall/Syscall.bd",
        "packages/foundation/src/Core/Syscall/Descriptor.bd",
        "packages/foundation/src/Core/Syscall/ReadRequest.bd",
        "packages/foundation/src/Core/Syscall/ReadLimit.bd",
        "packages/foundation/src/Core/Syscall/StandardStream.bd",
        "packages/foundation/src/Core/Syscall/SyscallError.bd",
        "packages/foundation/src/Core/Syscall/WriteRequest.bd",
        "packages/foundation/src/Core/Time/Time.bd",
        "packages/foundation/src/Core/Time/Instant.bd",
        "packages/foundation/src/Core/Time/Duration.bd",
        "packages/foundation/src/Core/Time/Date.bd",
        "packages/foundation/src/Core/Time/TimeOfDay.bd",
        "packages/foundation/src/Core/Time/DateTime.bd",
        "packages/foundation/src/Core/Time/TimeError.bd",
        "packages/runtime/src/Runtime/Abi.bd",
        "packages/runtime/src/Runtime/Init.bd",
        "packages/compiler-sdk/src/Beskid/Syntax.bd",
        "packages/compiler-sdk/src/Beskid/Syntax/Nodes.bd",
        "packages/compiler-sdk/src/Beskid/Compiler/Emitter.bd",
        "packages/compiler-sdk/src/Beskid/Compiler/Emitter/Kind.bd",
        "packages/compiler-sdk/src/Beskid/Compiler/Emitter/Nodes.bd",
        "packages/compiler-sdk/src/Beskid/Compiler/Emitter/Contracts.bd",
        "packages/compiler-sdk/src/Beskid/Compiler/Emitter/Items.bd",
        "packages/compiler-sdk/src/Beskid/Compiler/Emitter/Contribution.bd",
        "packages/compiler-sdk/src/Beskid/Compiler/Collect.bd",
        "packages/compiler-sdk/src/Beskid/Compiler/Compilation.bd",
    ]
}

mod compile;
mod layout;
