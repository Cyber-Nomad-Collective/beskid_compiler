use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::projects::{CompilePlan, ProjectModSection};

use super::invoker::{AnalyzerOutcome, CollectorOutcome, GeneratorOutcome, RewriterOutcome};

#[derive(Debug, Clone)]
pub(crate) struct DiscoveredMod {
    pub(crate) dependency_name: String,
    pub(crate) project_name: String,
    pub(crate) project_root: PathBuf,
    pub(crate) manifest_path: PathBuf,
    pub(crate) source_root: PathBuf,
    pub(crate) mod_section: Option<ProjectModSection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractRegistration {
    pub contract_id: String,
    pub type_id: String,
    pub entry_symbol: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModArtifactDescriptor {
    pub schema_version: u32,
    pub package_id: String,
    #[serde(default)]
    pub package_version: Option<String>,
    pub mod_source_hash: String,
    pub lock_hash: String,
    pub target_triple: String,
    pub compiler_version: String,
    pub object_file: String,
    #[serde(default)]
    pub registrations: Vec<ContractRegistration>,
    #[serde(skip)]
    pub artifact_dir: PathBuf,
}

impl ModArtifactDescriptor {
    /// Absolute native object path relative to this descriptor's artifact directory.
    pub fn object_path(&self) -> PathBuf {
        self.artifact_dir.join(&self.object_file)
    }

    /// Absolute sidecar JSON path inside this descriptor's artifact directory. Mirrors the
    /// path that `beskid_aot::mod_artifact` writes when packing a Mod project.
    pub fn sidecar_path(&self) -> PathBuf {
        self.artifact_dir.join("mod.descriptor.json")
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LoadedModArtifact {
    pub(crate) discovered: DiscoveredMod,
    pub(crate) descriptor: Option<ModArtifactDescriptor>,
    pub(crate) registrations: Vec<ContractRegistration>,
}

#[derive(Debug, Clone, Default)]
pub struct ModHostSession {
    loaded: Vec<LoadedModArtifact>,
    composition_snapshot: Option<crate::composition::CompositionSnapshot>,
}

impl ModHostSession {
    pub(crate) fn new(loaded: Vec<LoadedModArtifact>) -> Self {
        Self {
            loaded,
            composition_snapshot: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.loaded.is_empty()
    }

    pub fn registrations(&self) -> impl Iterator<Item = &ContractRegistration> {
        self.loaded
            .iter()
            .flat_map(|artifact| artifact.registrations.iter())
    }

    pub fn loaded_descriptor_count(&self) -> usize {
        self.loaded
            .iter()
            .filter(|artifact| artifact.descriptor.is_some())
            .count()
    }

    pub fn set_composition_snapshot(&mut self, snapshot: crate::composition::CompositionSnapshot) {
        self.composition_snapshot = Some(snapshot);
    }

    pub fn composition_snapshot(&self) -> Option<&crate::composition::CompositionSnapshot> {
        self.composition_snapshot.as_ref()
    }

    pub fn composition_snapshot_or_default(&self) -> crate::composition::CompositionSnapshot {
        self.composition_snapshot.clone().unwrap_or_default()
    }
}

pub struct ModHostInput<'a> {
    pub compile_plan: Option<&'a CompilePlan>,
    pub source_name: &'a str,
    pub source: &'a str,
    pub pipeline: Option<&'a dyn beskid_pipeline::PipelineObserver>,
    /// Optional contract invoker. When `None`, the host installs a default
    /// [`crate::mod_host::StubContractInvoker`] so all four contract phases still
    /// dispatch deterministically. Tests can pass a recording invoker to assert
    /// `(contractId, typeId, entrySymbol)` tuples were dispatched.
    pub invoker: Option<&'a dyn super::invoker::ContractInvoker>,
}

pub struct ModHostGenerateResult {
    pub program: crate::syntax::Spanned<crate::syntax::Program>,
    pub session: ModHostSession,
    /// Diagnostics from `macro.expand` (including registry issues and residual invocations).
    pub macro_diagnostics: Vec<crate::analysis::SemanticDiagnostic>,
    /// Outcomes returned by `Collector` invocations during `mod.collect`.
    pub collector_outcomes: Vec<CollectorOutcome>,
    /// Outcomes returned by `Generator` invocations during `mod.generate`.
    pub generator_outcomes: Vec<GeneratorOutcome>,
}

pub struct ModHostAnalyzeResult {
    pub program: crate::syntax::Spanned<crate::syntax::Program>,
    /// Outcomes returned by `Analyzer` invocations during `mod.analyze`.
    pub analyzer_outcomes: Vec<AnalyzerOutcome>,
    /// Outcomes returned by `Rewriter` invocations during `mod.rewrite`.
    pub rewriter_outcomes: Vec<RewriterOutcome>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CollectedContracts {
    pub(crate) registrations: Vec<ContractRegistration>,
    pub(crate) outcomes: Vec<CollectorOutcome>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct GeneratedSyntax {
    pub(crate) registrations: Vec<ContractRegistration>,
    pub(crate) contributions: Vec<String>,
    pub(crate) outcomes: Vec<GeneratorOutcome>,
}

impl GeneratedSyntax {
    pub(crate) fn requires_reparse(&self) -> bool {
        !self.contributions.is_empty() || !self.registrations.is_empty()
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct AnalyzedContracts {
    pub(crate) registrations: Vec<ContractRegistration>,
    pub(crate) outcomes: Vec<AnalyzerOutcome>,
}

#[derive(Debug, Clone)]
pub(crate) struct RewriteResult {
    pub(crate) program: crate::syntax::Spanned<crate::syntax::Program>,
    pub(crate) registrations: Vec<ContractRegistration>,
    pub(crate) outcomes: Vec<RewriterOutcome>,
}

#[cfg(test)]
mod tests {
    use super::ModHostSession;

    #[test]
    fn composition_snapshot_defaults_when_unset() {
        let session = ModHostSession::default();
        let snapshot = session.composition_snapshot_or_default();
        assert!(snapshot.launched_host.is_empty());
        assert!(snapshot.registrations.is_empty());
    }

    #[test]
    fn composition_snapshot_roundtrips_when_set() {
        let mut session = ModHostSession::default();
        let snapshot = crate::composition::CompositionSnapshot {
            version: 1,
            launched_host: "AppHost".to_string(),
            launch_span: None,
            registrations: Vec::new(),
            scope_names: std::collections::HashMap::new(),
        };
        session.set_composition_snapshot(snapshot.clone());
        assert_eq!(session.composition_snapshot(), Some(&snapshot));
        assert_eq!(session.composition_snapshot_or_default(), snapshot);
    }
}
