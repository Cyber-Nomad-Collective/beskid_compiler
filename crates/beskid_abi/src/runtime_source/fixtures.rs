//! Compiler-owned runtime test inventory, separate from the production source corpus.

use std::path::{Path, PathBuf};

use super::{RuntimeCapabilityError, RuntimeIntrinsicCapability, canonical_runtime_intrinsic_capability};
use crate::abi_v5::{AbiManifestV5, SourceUnit};

const CANONICAL_RUNTIME_FIXTURE_MANIFEST: &str =
    include_str!("../../../../runtime/beskid/tests/runtime_semantics/runtime_semantics.bproj");

/// Non-serializable authority for one exact, compiler-declared runtime fixture.
/// The physical project, its entire manifest, and the fixture text must be canonical.
/// Package names, matching basenames, and copied sources cannot construct this proof.
#[derive(Debug)]
pub struct RuntimeFixtureProof {
    fixture: SourceUnit,
}

pub fn runtime_fixture_project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime/beskid/tests/runtime_semantics")
}

pub fn canonical_runtime_fixture_sources() -> Vec<SourceUnit> {
    [
        ("LifecycleTests", include_str!("../../../../runtime/beskid/tests/runtime_semantics/src/LifecycleTests.bd")),
        (
            "ExternalWorkTests",
            include_str!("../../../../runtime/beskid/tests/runtime_semantics/src/ExternalWorkTests.bd"),
        ),
        (
            "NetworkNativeTests",
            include_str!("../../../../runtime/beskid/tests/runtime_semantics/src/NetworkNativeTests.bd"),
        ),
        (
            "CompositionTests",
            include_str!("../../../../runtime/beskid/tests/runtime_semantics/src/CompositionTests.bd"),
        ),
        ("GcTests", include_str!("../../../../runtime/beskid/tests/runtime_semantics/src/GcTests.bd")),
        ("ProcessIoTests", include_str!("../../../../runtime/beskid/tests/runtime_semantics/src/ProcessIoTests.bd")),
        ("SchedulerTests", include_str!("../../../../runtime/beskid/tests/runtime_semantics/src/SchedulerTests.bd")),
    ]
    .into_iter()
    .map(|(name, source)| SourceUnit {
        logical_path: format!("tests/runtime_semantics/src/{name}.bd"),
        source: source.to_owned(),
    })
    .collect()
}

/// Admit only a declared target in this compiler checkout, with unchanged embedded manifest
/// and fixture text. A copied project is ordinary source, even with identical package metadata.
pub fn prove_runtime_fixture(
    manifest: &Path,
    target: &str,
    entry: &str,
    source: &str,
) -> Result<Option<RuntimeFixtureProof>, RuntimeCapabilityError> {
    let expected = runtime_fixture_project_root().join("runtime_semantics.bproj");
    if manifest.canonicalize().ok().zip(expected.canonicalize().ok()).is_none_or(|(a, b)| a != b) {
        return Ok(None);
    }
    let manifest_source = std::fs::read_to_string(manifest).map_err(|_| RuntimeCapabilityError::SourceSetMismatch)?;
    prove_runtime_fixture_contents(&manifest_source, target, entry, source)
}

fn prove_runtime_fixture_contents(
    manifest_source: &str,
    target: &str,
    entry: &str,
    source: &str,
) -> Result<Option<RuntimeFixtureProof>, RuntimeCapabilityError> {
    if manifest_source != CANONICAL_RUNTIME_FIXTURE_MANIFEST {
        return Err(RuntimeCapabilityError::SourceSetMismatch);
    }
    let Some(fixture) = canonical_runtime_fixture_sources()
        .into_iter()
        .find(|fixture| fixture.logical_path == format!("tests/runtime_semantics/src/{target}.bd"))
    else {
        return Ok(None);
    };
    if entry != format!("{target}.bd") || source != fixture.source {
        return Err(RuntimeCapabilityError::SourceSetMismatch);
    }
    Ok(Some(RuntimeFixtureProof { fixture }))
}

impl RuntimeFixtureProof {
    pub fn fixture(&self) -> &SourceUnit {
        &self.fixture
    }

    /// The generated ABI declarations (e.g. AbiValue's layout prelude) are checked in as part of
    /// their owning source file, not assembled separately by the compiler, so every source-owned
    /// byte must equal the on-disk text -- no per-file stripping required.
    pub fn source_file_text<'a>(&self, source: &'a SourceUnit) -> &'a str {
        &source.source
    }

    pub fn intrinsic_capability(
        &self,
        manifest: &AbiManifestV5,
    ) -> Result<RuntimeIntrinsicCapability, RuntimeCapabilityError> {
        let mut capability = canonical_runtime_intrinsic_capability(manifest)?;
        capability.authorize_fixture(self.fixture.logical_path.clone());
        Ok(capability)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copied_project_cannot_claim_fixture_authority() {
        let fixture = canonical_runtime_fixture_sources().remove(0);
        assert!(
            prove_runtime_fixture(
                Path::new("/untrusted/runtime_semantics.bproj"),
                "LifecycleTests",
                "LifecycleTests.bd",
                &fixture.source
            )
            .unwrap()
            .is_none()
        );
    }

    #[test]
    fn changed_fixture_or_entry_is_rejected() {
        let manifest = runtime_fixture_project_root().join("runtime_semantics.bproj");
        let fixture = canonical_runtime_fixture_sources().remove(0);
        assert!(
            prove_runtime_fixture(&manifest, "LifecycleTests", "LifecycleTests.bd", &fixture.source).unwrap().is_some()
        );
        assert!(prove_runtime_fixture(&manifest, "LifecycleTests", "Other.bd", &fixture.source).is_err());
        assert!(prove_runtime_fixture(&manifest, "LifecycleTests", "LifecycleTests.bd", "test forged {}").is_err());
    }

    #[test]
    fn changed_manifest_dependency_cannot_retain_fixture_authority() {
        let fixture = canonical_runtime_fixture_sources().remove(0);
        let changed = CANONICAL_RUNTIME_FIXTURE_MANIFEST.replace("path = \"../..\"", "path = \"../../untrusted\"");
        assert_ne!(changed, CANONICAL_RUNTIME_FIXTURE_MANIFEST);
        assert!(
            prove_runtime_fixture_contents(&changed, "LifecycleTests", "LifecycleTests.bd", &fixture.source).is_err()
        );
    }

    #[test]
    fn changed_manifest_target_cannot_retain_fixture_authority() {
        let fixture = canonical_runtime_fixture_sources().remove(0);
        // Keep the selected target name, entry, and source intact: target metadata is also
        // part of the project proof, not authority supplied by the caller's parsed plan.
        let changed = CANONICAL_RUNTIME_FIXTURE_MANIFEST.replacen("kind = Lib", "kind = Exe", 1);
        assert_ne!(changed, CANONICAL_RUNTIME_FIXTURE_MANIFEST);
        assert!(
            prove_runtime_fixture_contents(&changed, "LifecycleTests", "LifecycleTests.bd", &fixture.source).is_err()
        );
    }
}
