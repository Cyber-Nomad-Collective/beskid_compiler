use std::path::{Path, PathBuf};

pub(super) fn stdlib_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest_dir.join("../../corelib/standard_library"),
        manifest_dir.join("../../../corelib/standard_library"),
        manifest_dir.join("../../../../corelib/standard_library"),
    ];

    candidates
        .into_iter()
        .find(|root| root.join("Project.proj").is_file())
        .unwrap_or_else(|| {
            panic!(
                "canonical stdlib root not found. expected corelib/standard_library near {}",
                manifest_dir.display()
            )
        })
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
