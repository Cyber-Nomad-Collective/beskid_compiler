use std::path::{Path, PathBuf};

pub(super) fn stdlib_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir.join("../../corelib/standard_library");
    if root.join("Project.proj").is_file() {
        return root;
    }
    panic!(
        "canonical stdlib root not found. expected compiler/corelib/standard_library (init the corelib submodule); looked near {}",
        manifest_dir.display()
    );
}

pub(super) fn expected_stdlib_files() -> [&'static str; 5] {
    [
        "Core/Results.bd",
        "Core/ErrorHandling.bd",
        "Core/String.bd",
        "System/IO.bd",
        "Prelude.bd",
    ]
}

mod compile;
mod layout;
