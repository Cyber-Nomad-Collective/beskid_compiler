use std::collections::HashSet;

use anyhow::{Result, bail};

use crate::projects::MOD_CAPABILITY_NAMES;

use super::types::{ContractRegistration, LoadedModArtifact};

pub(crate) fn enforce_capabilities(loaded: &[LoadedModArtifact]) -> Result<()> {
    for artifact in loaded {
        let Some(mod_section) = artifact.discovered.mod_section.as_ref() else {
            continue;
        };
        let Some(capabilities) = mod_section.capabilities.as_ref() else {
            continue;
        };

        for capability in capabilities {
            if !MOD_CAPABILITY_NAMES.iter().any(|known| known == capability) {
                bail!(
                    "unknown `project.mod.capabilities` entry `{capability}` in {}",
                    artifact.discovered.manifest_path.display()
                );
            }
        }

        let available = capabilities
            .iter()
            .map(String::as_str)
            .collect::<HashSet<_>>();
        for registration in &artifact.registrations {
            if let Some(required) = required_capability(registration)
                && !available.contains(required)
            {
                bail!(
                    "mod `{}` registers `{}` but is missing required capability `{}`",
                    artifact.discovered.project_name,
                    registration.contract_id,
                    required
                );
            }
        }
    }

    Ok(())
}

fn required_capability(registration: &ContractRegistration) -> Option<&'static str> {
    let contract = registration.contract_id.rsplit('.').next()?;
    match contract {
        "Collector" => Some("read_project_sources"),
        "Generator" | "AttributeGenerator" => Some("emit_syntax"),
        "Analyzer" => Some("query_semantic_snapshot"),
        "Rewriter" => Some("rewrite_syntax"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::projects::ProjectModSection;

    use super::*;
    use crate::mod_host::types::{DiscoveredMod, LoadedModArtifact};

    #[test]
    fn enforces_required_capability_for_registered_generator() {
        let loaded = vec![loaded_with_caps(
            Some(vec!["read_project_sources".to_owned()]),
            ContractRegistration {
                contract_id: "Beskid.Compiler.Collect.Generator".to_owned(),
                type_id: "Mod.Emit".to_owned(),
                entry_symbol: "emit".to_owned(),
            },
        )];

        let err = enforce_capabilities(&loaded).expect_err("missing emit_syntax should fail");
        assert!(err.to_string().contains("emit_syntax"));
    }

    #[test]
    fn allows_empty_or_unregistered_capabilities_for_mvp() {
        let loaded = vec![LoadedModArtifact {
            discovered: discovered(None),
            descriptor: None,
            registrations: Vec::new(),
        }];

        enforce_capabilities(&loaded).expect("empty registrations are allowed");
    }

    fn loaded_with_caps(
        capabilities: Option<Vec<String>>,
        registration: ContractRegistration,
    ) -> LoadedModArtifact {
        LoadedModArtifact {
            discovered: discovered(capabilities),
            descriptor: None,
            registrations: vec![registration],
        }
    }

    fn discovered(capabilities: Option<Vec<String>>) -> DiscoveredMod {
        DiscoveredMod {
            dependency_name: "Mod".to_owned(),
            project_name: "Mod".to_owned(),
            project_root: PathBuf::from("/mod"),
            manifest_path: PathBuf::from("/mod/Project.proj"),
            source_root: PathBuf::from("/mod/Src"),
            mod_section: Some(ProjectModSection {
                max_generator_rounds: None,
                capabilities,
                artifact_policy: None,
            }),
        }
    }
}
