use std::path::PathBuf;

use beskid_abi::runtime_kit::BuildProfile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeKitProfile {
    Debug,
    Release,
}

impl RuntimeKitProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Release => "release",
        }
    }
}

impl From<RuntimeKitProfile> for BuildProfile {
    fn from(value: RuntimeKitProfile) -> Self {
        match value {
            RuntimeKitProfile::Debug => Self::Debug,
            RuntimeKitProfile::Release => Self::Release,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeKitBuildOptions {
    pub prefix: PathBuf,
    pub target: String,
    pub profile: RuntimeKitProfile,
    pub static_library: PathBuf,
    pub shared_library: PathBuf,
    pub shared_import_library: Option<PathBuf>,
}

/// The pair of native artifacts required for one optimization profile.
///
/// This intentionally carries only native files emitted by the runtime build.  The publisher
/// derives the source identity from the compiler-embedded canonical corpus; callers cannot use
/// this matrix API to label an arbitrary bridge or host runtime as ABI-v5.
#[derive(Debug, Clone)]
pub struct RuntimeKitProfileArtifacts {
    pub profile: RuntimeKitProfile,
    pub static_library: PathBuf,
    pub shared_library: PathBuf,
    pub shared_import_library: Option<PathBuf>,
    /// Platform-adapter output for the exact static archive's defined/undefined symbols.
    pub static_provenance_symbol_list: PathBuf,
    /// Platform-adapter output for the exact shared library's defined/undefined symbols.
    pub shared_provenance_symbol_list: PathBuf,
}

/// One target's complete debug/release publication request.
///
/// Both profiles are staged and published as one immutable target subtree under the same prefix.
/// The layout is target-neutral, so CI can validate Darwin and Windows publication paths on a
/// Linux host while only executing native JIT/link smokes on the matching host.
#[derive(Debug, Clone)]
pub struct RuntimeKitMatrixBuildOptions {
    pub prefix: PathBuf,
    pub target: String,
    pub profiles: Vec<RuntimeKitProfileArtifacts>,
}
