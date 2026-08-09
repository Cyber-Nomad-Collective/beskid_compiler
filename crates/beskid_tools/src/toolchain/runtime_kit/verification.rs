use anyhow::{Result, anyhow};
use beskid_abi::abi_v5::TargetMetadata;
use beskid_abi::runtime_provenance::{RuntimeProvenanceAudit, parse_symbol_list};

#[derive(Debug, Clone, Copy)]
pub(super) enum RuntimeArtifactKind {
    StaticArchive,
    SharedLibrary,
}

pub(super) fn verify_provenance_symbol_list(
    target: &str,
    path: &std::path::Path,
    artifact_kind: RuntimeArtifactKind,
) -> Result<()> {
    let source = std::fs::read_to_string(path)
        .map_err(|error| anyhow!("read ABI-v5 runtime provenance symbol list `{}`: {error}", path.display()))?;
    let symbols = parse_symbol_list(&source)
        .map_err(|error| anyhow!("parse ABI-v5 runtime provenance symbol list `{}`: {error}", path.display()))?;
    let target_metadata = TargetMetadata::supported()
        .into_iter()
        .find(|candidate| candidate.triple.as_str() == target)
        .ok_or_else(|| anyhow!("unsupported ABI-v5 runtime target `{target}`"))?;
    let audit = RuntimeProvenanceAudit::canonical(target_metadata)
        .map_err(|error| anyhow!("derive ABI-v5 provenance audit for `{target}`: {error:?}"))?;
    let verification = match artifact_kind {
        RuntimeArtifactKind::StaticArchive => audit.verify_static_archive(&symbols),
        RuntimeArtifactKind::SharedLibrary => audit.verify_shared(&symbols),
    };
    verification.map_err(|error| anyhow!("ABI-v5 runtime provenance rejected `{}`: {error}", path.display()))
}
