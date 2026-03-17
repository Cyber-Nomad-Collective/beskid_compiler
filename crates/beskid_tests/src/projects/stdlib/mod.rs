use std::path::{Path, PathBuf};

pub(super) fn stdlib_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../standard_library")
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
