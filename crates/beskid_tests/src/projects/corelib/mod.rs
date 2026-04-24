use std::path::{Path, PathBuf};

pub(super) fn corelib_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    for root in corelib_source_candidates(manifest_dir) {
        if root.join("Project.proj").is_file() {
            return root;
        }
    }
    panic!(
        "canonical corelib root not found. expected compiler/corelib/beskid_corelib (init the corelib submodule); looked near {}",
        manifest_dir.display()
    );
}

fn corelib_source_candidates(manifest_dir: &Path) -> [PathBuf; 1] {
    [manifest_dir.join("../../corelib/beskid_corelib")]
}

pub(super) fn expected_corelib_files() -> &'static [&'static str] {
    &[
        "Collections/Array.bd",
        "Collections/List.bd",
        "Collections/Map.bd",
        "Collections/Queue.bd",
        "Collections/Set.bd",
        "Collections/Stack.bd",
        "Core/ErrorHandling.bd",
        "Core/Results.bd",
        "Core/String.bd",
        "Prelude.bd",
        "Query/Contracts.bd",
        "Query/Execution.bd",
        "Query/Operators.bd",
        "System/Environment.bd",
        "System/FS.bd",
        "System/IO.bd",
        "System/Path.bd",
        "System/Process.bd",
        "System/Syscall.bd",
        "System/Time.bd",
        "Testing/Assertions.bd",
        "Testing/Contracts.bd",
    ]
}

mod compile;
mod layout;
