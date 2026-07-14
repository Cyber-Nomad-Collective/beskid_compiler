//! Hermetic production of installed ABI-v5 runtime kits.

use std::path::PathBuf;

use anyhow::{Result, anyhow, bail};
use beskid_abi::abi_v5::TargetMetadata;
use beskid_abi::runtime_kit::{
    BuildProfile, ResolvedRuntimeKit, RuntimeKitBuildRequest, build_runtime_kit,
};

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
    pub source_hash: String,
    pub static_library: PathBuf,
    pub shared_library: PathBuf,
    pub shared_import_library: Option<PathBuf>,
}

pub fn build(options: RuntimeKitBuildOptions) -> Result<ResolvedRuntimeKit> {
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|candidate| candidate.triple.as_str() == options.target)
        .ok_or_else(|| {
            let supported = TargetMetadata::supported()
                .into_iter()
                .map(|target| target.triple.as_str().to_owned())
                .collect::<Vec<_>>()
                .join(", ");
            anyhow!(
                "unsupported ABI-v5 runtime target `{}`; expected one of: {supported}",
                options.target
            )
        })?;
    if options.source_hash.trim() != options.source_hash {
        bail!("runtime source hash must not contain surrounding whitespace");
    }
    let request = RuntimeKitBuildRequest {
        prefix: options.prefix,
        target,
        profile: options.profile.into(),
        runtime_source_hash: options.source_hash,
        static_library: options.static_library,
        shared_library: options.shared_library,
        shared_import_library: options.shared_import_library,
    };
    build_runtime_kit(&request)
        .map_err(|error| anyhow!("failed to build ABI-v5 runtime kit: {error:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> RuntimeKitBuildOptions {
        RuntimeKitBuildOptions {
            prefix: "/tmp/beskid-runtime-kit-negative".into(),
            target: "not-a-supported-target".into(),
            profile: RuntimeKitProfile::Debug,
            source_hash: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
            static_library: "missing-static".into(),
            shared_library: "missing-shared".into(),
            shared_import_library: None,
        }
    }

    #[test]
    fn rejects_unsupported_target_before_reading_artifacts() {
        let error = build(options()).unwrap_err().to_string();
        assert!(error.contains("unsupported ABI-v5 runtime target"));
        assert!(error.contains("x86_64-unknown-linux-gnu"));
    }

    #[test]
    fn rejects_source_hash_with_hidden_surrounding_whitespace() {
        let mut options = options();
        options.target = "x86_64-unknown-linux-gnu".into();
        options.source_hash.push('\n');
        let error = build(options).unwrap_err().to_string();
        assert!(error.contains("source hash must not contain surrounding whitespace"));
    }
}
