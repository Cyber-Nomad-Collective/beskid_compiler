use anyhow::{Result, anyhow};
use beskid_abi::abi_v5::TargetMetadata;
use beskid_abi::runtime_kit::{ResolvedRuntimeKit, RuntimeKitBuildRequest};
use beskid_abi::runtime_source::{build_canonical_runtime_kit, canonical_runtime_source_hash};

use super::model::RuntimeKitBuildOptions;

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
            anyhow!("unsupported ABI-v5 runtime target `{}`; expected one of: {supported}", options.target)
        })?;
    let canonical_hash = canonical_runtime_source_hash();
    let request = RuntimeKitBuildRequest {
        prefix: options.prefix,
        target,
        profile: options.profile.into(),
        runtime_source_hash: canonical_hash,
        static_library: options.static_library,
        shared_library: options.shared_library,
        shared_import_library: options.shared_import_library,
    };
    build_canonical_runtime_kit(&request).map_err(|error| anyhow!("failed to build ABI-v5 runtime kit: {error:?}"))
}
