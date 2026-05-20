use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::projects::{CompilePlan, ProjectModSection};

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

#[derive(Debug, Clone)]
pub(crate) struct LoadedModArtifact {
    pub(crate) discovered: DiscoveredMod,
    pub(crate) descriptor: Option<ModArtifactDescriptor>,
    pub(crate) registrations: Vec<ContractRegistration>,
}

#[derive(Debug, Clone, Default)]
pub struct ModHostSession {
    loaded: Vec<LoadedModArtifact>,
}

impl ModHostSession {
    pub(crate) fn new(loaded: Vec<LoadedModArtifact>) -> Self {
        Self { loaded }
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
}

pub struct ModHostInput<'a> {
    pub compile_plan: Option<&'a CompilePlan>,
    pub source_name: &'a str,
    pub source: &'a str,
    pub pipeline: Option<&'a dyn beskid_pipeline::PipelineObserver>,
}

pub struct ModHostGenerateResult {
    pub program: crate::syntax::Spanned<crate::syntax::Program>,
    pub session: ModHostSession,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CollectedContracts {
    pub(crate) registrations: Vec<ContractRegistration>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct GeneratedSyntax {
    pub(crate) registrations: Vec<ContractRegistration>,
    pub(crate) contributions: Vec<String>,
}

impl GeneratedSyntax {
    pub(crate) fn requires_reparse(&self) -> bool {
        !self.contributions.is_empty() || !self.registrations.is_empty()
    }
}
