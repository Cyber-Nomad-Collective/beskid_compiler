use std::path::{Path, PathBuf};

pub(super) fn stdlib_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest_dir.join("../../../standard_library"),
        manifest_dir.join("../../../corelib/standard_library"),
        manifest_dir.join("../../../../standard_library"),
        manifest_dir.join("../../../../corelib/standard_library"),
    ];

    candidates
        .into_iter()
        .find(|root| root.join("Project.proj").is_file())
        .unwrap_or_else(|| manifest_dir.join("../../../standard_library"))
}

pub(super) fn expected_stdlib_files() -> [&'static str; 18] {
    [
        "Core/Results.bd",
        "Core/ErrorHandling.bd",
        "Core/String.bd",
        "Collections/Array.bd",
        "Collections/List.bd",
        "Collections/Map.bd",
        "Collections/Set.bd",
        "Collections/Queue.bd",
        "Collections/Stack.bd",
        "Query/Contracts.bd",
        "Query/Operators.bd",
        "Query/Execution.bd",
        "System/IO.bd",
        "System/FS.bd",
        "System/Path.bd",
        "System/Time.bd",
        "System/Environment.bd",
        "System/Process.bd",
    ]
}

mod compile;
mod layout;
