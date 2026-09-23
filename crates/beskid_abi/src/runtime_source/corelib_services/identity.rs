use super::super::sources::{
    CANONICAL_CORELIB_ARGS_SOURCE_PATH, CANONICAL_CORELIB_CHANNEL_SOURCE_PATH,
    CANONICAL_CORELIB_CONCURRENCY_SOURCE_PATH, CANONICAL_CORELIB_CONSOLE_LINUX_SOURCE_PATH,
    CANONICAL_CORELIB_CONSOLE_MACOS_SOURCE_PATH, CANONICAL_CORELIB_CONSOLE_TERMINAL_SOURCE_PATH,
    CANONICAL_CORELIB_CONSOLE_WINDOWS_SOURCE_PATH, CANONICAL_CORELIB_FIBER_SOURCE_PATH,
    CANONICAL_CORELIB_FS_SOURCE_PATH, CANONICAL_CORELIB_HUB_SOURCE_PATH, CANONICAL_CORELIB_MUTEX_SOURCE_PATH,
    CANONICAL_CORELIB_SYSCALL_SOURCE_PATH, CANONICAL_CORELIB_WAIT_GROUP_SOURCE_PATH,
    CANONICAL_FOUNDATION_ARRAY_SOURCE_PATH, CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH,
    CANONICAL_FOUNDATION_BYTES_SLICE_SOURCE_PATH, CANONICAL_FOUNDATION_ENVIRONMENT_SOURCE_PATH,
    CANONICAL_FOUNDATION_ERROR_SOURCE_PATH, CANONICAL_FOUNDATION_OUTPUT_SOURCE_PATH,
    CANONICAL_FOUNDATION_PATH_SOURCE_PATH, CANONICAL_FOUNDATION_PROCESS_SOURCE_PATH,
    CANONICAL_FOUNDATION_RANDOM_SOURCE_PATH, CANONICAL_FOUNDATION_STRING_CORE_SOURCE_PATH,
    CANONICAL_FOUNDATION_STRING_UTF8_SOURCE_PATH, CANONICAL_FOUNDATION_TEXT_CURSOR_SOURCE_PATH,
    CANONICAL_FOUNDATION_TIME_SOURCE_PATH, CANONICAL_NETWORK_INTERNAL_SOURCE_PATH,
};

/// The canonical compiler-owned source file for one Foundation service unit.
///
/// Authority is tied to this checked-in file identity as well as embedded bytes and logical
/// module path. A user project that copies `Testing/Assert.bd` cannot acquire it.
///
/// The returned path is the canonical physical identity used by source assembly. Failure to
/// resolve the checked-in file fails closed rather than granting authority to a lexical alias.
pub fn canonical_corelib_service_source_path(logical_path: &str) -> Option<std::path::PathBuf> {
    Some(corelib_service_source_identity(logical_path)?.canonical_path)
}

/// Declared compiler location and resolved physical identity from the one source inventory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorelibServiceSourceIdentity {
    pub declared_path: std::path::PathBuf,
    pub canonical_path: std::path::PathBuf,
}

/// Resolve both source identities without resolving a caller-supplied origin.
pub fn corelib_service_source_identity(logical_path: &str) -> Option<CorelibServiceSourceIdentity> {
    let (package, relative) = match logical_path {
        CANONICAL_CORELIB_SYSCALL_SOURCE_PATH => ("foundation", "Core/Syscall/Syscall.bd"),
        CANONICAL_CORELIB_ARGS_SOURCE_PATH => ("foundation", "Core/Args/Args.bd"),
        CANONICAL_CORELIB_CONCURRENCY_SOURCE_PATH => ("concurrency", "Concurrency.bd"),
        CANONICAL_CORELIB_FIBER_SOURCE_PATH => ("concurrency", "Concurrency/Fiber.bd"),
        CANONICAL_CORELIB_CONSOLE_LINUX_SOURCE_PATH => ("console", "Platform/Linux.bd"),
        CANONICAL_CORELIB_CONSOLE_MACOS_SOURCE_PATH => ("console", "Platform/MacOS.bd"),
        CANONICAL_CORELIB_CONSOLE_WINDOWS_SOURCE_PATH => ("console", "Platform/Windows.bd"),
        CANONICAL_CORELIB_CONSOLE_TERMINAL_SOURCE_PATH => ("console", "Platform/Terminal.bd"),
        CANONICAL_CORELIB_FS_SOURCE_PATH => ("foundation", "Core/FS/FS.bd"),
        CANONICAL_CORELIB_CHANNEL_SOURCE_PATH => ("concurrency", "Concurrency/Channel.bd"),
        CANONICAL_CORELIB_MUTEX_SOURCE_PATH => ("concurrency", "Concurrency/Mutex.bd"),
        CANONICAL_CORELIB_HUB_SOURCE_PATH => ("concurrency", "Concurrency/Hub.bd"),
        CANONICAL_CORELIB_WAIT_GROUP_SOURCE_PATH => ("concurrency", "Concurrency/WaitGroup.bd"),
        CANONICAL_FOUNDATION_ARRAY_SOURCE_PATH => ("foundation", "Core/Collections/Array.bd"),
        CANONICAL_FOUNDATION_BYTES_SLICE_SOURCE_PATH => ("foundation", "Core/Bytes/Slice.bd"),
        CANONICAL_FOUNDATION_ENVIRONMENT_SOURCE_PATH => ("foundation", "Core/Environment/Environment.bd"),
        CANONICAL_FOUNDATION_PATH_SOURCE_PATH => ("foundation", "Core/Path/Path.bd"),
        CANONICAL_FOUNDATION_PROCESS_SOURCE_PATH => ("foundation", "Core/Process/Process.bd"),
        CANONICAL_FOUNDATION_RANDOM_SOURCE_PATH => ("foundation", "Core/Random/Random.bd"),
        CANONICAL_FOUNDATION_STRING_CORE_SOURCE_PATH => ("foundation", "Core/String/Core.bd"),
        CANONICAL_FOUNDATION_STRING_UTF8_SOURCE_PATH => ("foundation", "Core/String/Utf8.bd"),
        CANONICAL_FOUNDATION_TEXT_CURSOR_SOURCE_PATH => ("foundation", "Core/Text/Cursor.bd"),
        CANONICAL_FOUNDATION_TIME_SOURCE_PATH => ("foundation", "Core/Time/Time.bd"),
        CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH => ("foundation", "Testing/Assert.bd"),
        CANONICAL_FOUNDATION_OUTPUT_SOURCE_PATH => ("foundation", "Core/Output/Output.bd"),
        CANONICAL_FOUNDATION_ERROR_SOURCE_PATH => ("foundation", "Core/Error/Error.bd"),
        CANONICAL_NETWORK_INTERNAL_SOURCE_PATH => ("network", "Network/Internal.bd"),
        _ => return None,
    };
    let declared_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()?
        .parent()?
        .join("corelib/packages")
        .join(package)
        .join("src")
        .join(relative);
    let canonical_path = std::fs::canonicalize(&declared_path).ok()?;
    Some(CorelibServiceSourceIdentity { declared_path, canonical_path })
}

/// Compare source locations without filesystem resolution or parent traversal folding.
/// Only Windows drive/verbatim-drive prefixes are interchangeable; device and UNC namespaces
/// retain their exact component identity. This comparison alone grants no source authority.
pub fn corelib_source_locations_match(left: &std::path::Path, right: &std::path::Path) -> bool {
    let mut left = left.components();
    let mut right = right.components();
    loop {
        match (left.next(), right.next()) {
            (None, None) => return true,
            #[cfg(windows)]
            (Some(std::path::Component::Prefix(left)), Some(std::path::Component::Prefix(right))) => {
                use std::path::Prefix;
                match (left.kind(), right.kind()) {
                    (Prefix::Disk(a) | Prefix::VerbatimDisk(a), Prefix::Disk(b) | Prefix::VerbatimDisk(b))
                        if a == b => {}
                    _ if left == right => {}
                    _ => return false,
                }
            }
            (Some(left), Some(right)) if left == right => {}
            _ => return false,
        }
    }
}
