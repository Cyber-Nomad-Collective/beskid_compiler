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

pub(super) fn expected_corelib_files() -> [&'static str; 7] {
    [
        "Core/Results.bd",
        "Core/ErrorHandling.bd",
        "Core/String.bd",
        "Testing/Contracts.bd",
        "Testing/Assertions.bd",
        "System/IO.bd",
        "Prelude.bd",
    ]
}

mod compile;
mod layout;
