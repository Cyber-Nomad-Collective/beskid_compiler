//! Pre-`mod.collect` validation: detect conflicts and missing required fields across
//! all loaded mod artifacts and surface deterministic diagnostics.
//!
//! See `site/website/src/content/docs/platform-spec/compiler/compiler-mods/mod-host-bridge/`
//! for the normative spec. This pass runs after `mod.load` and **before** `mod.collect`;
//! any issues short-circuit scheduling.

use std::collections::BTreeMap;

use super::diagnostics::{ModHostDiagnostics, ModHostIssue};
use super::types::{ContractRegistration, LoadedModArtifact};

/// Closed set of SDK contract identifiers the host accepts. Mods that register any
/// other `contractId` value fail with **E1853** before scheduling. Suffix matching is
/// preserved for backwards compatibility with the existing `*.Collector` /
/// `*.Generator` filters used by collect / generate / analyze / rewrite phases.
const KNOWN_CONTRACT_SUFFIXES: &[&str] = &[
    "Collector",
    "Generator",
    "AttributeGenerator",
    "Analyzer",
    "Rewriter",
];

pub(crate) fn validate_registrations(
    loaded: &[LoadedModArtifact],
) -> Result<(), ModHostDiagnostics> {
    let mut issues = Vec::new();

    for artifact in loaded {
        validate_artifact(artifact, &mut issues);
    }
    validate_cross_artifact(loaded, &mut issues);

    issues.sort_by(|left, right| {
        left.code()
            .cmp(right.code())
            .then(left.message().cmp(&right.message()))
    });

    if issues.is_empty() {
        Ok(())
    } else {
        Err(ModHostDiagnostics::new(issues))
    }
}

fn validate_artifact(artifact: &LoadedModArtifact, issues: &mut Vec<ModHostIssue>) {
    let descriptor_path = artifact
        .descriptor
        .as_ref()
        .map(|descriptor| descriptor.sidecar_path())
        .unwrap_or_else(|| artifact.discovered.manifest_path.clone());
    let manifest_path = artifact.discovered.manifest_path.clone();
    let package_id = artifact
        .descriptor
        .as_ref()
        .map(|d| d.package_id.clone())
        .unwrap_or_else(|| artifact.discovered.project_name.clone());

    let mut seen: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    let mut has_analyzer = false;
    let mut rewriters: Vec<&ContractRegistration> = Vec::new();

    for (index, registration) in artifact.registrations.iter().enumerate() {
        if registration.entry_symbol.trim().is_empty() {
            issues.push(ModHostIssue::MissingEntrySymbol {
                package_id: package_id.clone(),
                contract_id: registration.contract_id.clone(),
                type_id: registration.type_id.clone(),
                descriptor: descriptor_path.clone(),
            });
        }
        if !is_known_contract(&registration.contract_id) {
            issues.push(ModHostIssue::UnknownContractId {
                package_id: package_id.clone(),
                contract_id: registration.contract_id.clone(),
                descriptor: descriptor_path.clone(),
            });
        }

        let key = (
            registration.contract_id.as_str(),
            registration.type_id.as_str(),
        );
        if seen.insert(key, index).is_some() {
            issues.push(ModHostIssue::DuplicateRegistrationInArtifact {
                package_id: package_id.clone(),
                contract_id: registration.contract_id.clone(),
                type_id: registration.type_id.clone(),
                descriptor: descriptor_path.clone(),
            });
        }

        if registration.contract_id.ends_with(".Analyzer") {
            has_analyzer = true;
        } else if registration.contract_id.ends_with(".Rewriter") {
            rewriters.push(registration);
        }
    }

    for rewriter in &rewriters {
        if !has_analyzer {
            issues.push(ModHostIssue::RewriterWithoutAnalyzer {
                package_id: package_id.clone(),
                type_id: rewriter.type_id.clone(),
                descriptor: descriptor_path.clone(),
            });
        }
    }

    if let Some(mod_section) = artifact.discovered.mod_section.as_ref()
        && let Some(capabilities) = mod_section.capabilities.as_ref() {
            if registrations_imply_required_contracts(artifact) && artifact.registrations.is_empty()
            {
                issues.push(ModHostIssue::EmptyRegistrationsForRequiredMod {
                    package_id: package_id.clone(),
                    manifest: manifest_path.clone(),
                });
            }
            let _ = capabilities; // capability mismatches are reported in `capabilities::enforce_capabilities`.
        }
}

fn registrations_imply_required_contracts(artifact: &LoadedModArtifact) -> bool {
    let Some(mod_section) = artifact.discovered.mod_section.as_ref() else {
        return false;
    };
    let Some(capabilities) = mod_section.capabilities.as_ref() else {
        return false;
    };
    capabilities
        .iter()
        .any(|capability| matches!(capability.as_str(), "emit_syntax" | "rewrite_syntax"))
}

fn validate_cross_artifact(loaded: &[LoadedModArtifact], issues: &mut Vec<ModHostIssue>) {
    let mut by_pair: BTreeMap<(&str, &str), Vec<String>> = BTreeMap::new();
    let mut by_entry_symbol: BTreeMap<&str, Vec<String>> = BTreeMap::new();

    for artifact in loaded {
        let package_id = artifact
            .descriptor
            .as_ref()
            .map(|d| d.package_id.as_str())
            .unwrap_or(artifact.discovered.project_name.as_str())
            .to_owned();
        for registration in &artifact.registrations {
            by_pair
                .entry((
                    registration.contract_id.as_str(),
                    registration.type_id.as_str(),
                ))
                .or_default()
                .push(package_id.clone());
            if !registration.entry_symbol.trim().is_empty() {
                by_entry_symbol
                    .entry(registration.entry_symbol.as_str())
                    .or_default()
                    .push(package_id.clone());
            }
        }
    }

    for ((contract_id, type_id), packages) in by_pair {
        let mut deduped = packages.clone();
        deduped.sort();
        deduped.dedup();
        if deduped.len() > 1 {
            issues.push(ModHostIssue::ConflictingRegistrationAcrossArtifacts {
                contract_id: contract_id.to_owned(),
                type_id: type_id.to_owned(),
                package_ids: deduped,
            });
        }
    }

    for (entry, packages) in by_entry_symbol {
        let mut deduped = packages.clone();
        deduped.sort();
        deduped.dedup();
        if deduped.len() > 1 {
            issues.push(ModHostIssue::DuplicateEntrySymbolAcrossArtifacts {
                entry_symbol: entry.to_owned(),
                package_ids: deduped,
            });
        }
    }
}

fn is_known_contract(contract_id: &str) -> bool {
    KNOWN_CONTRACT_SUFFIXES
        .iter()
        .any(|suffix| contract_id.ends_with(suffix))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::mod_host::types::{DiscoveredMod, LoadedModArtifact, ModArtifactDescriptor};
    use crate::projects::ProjectModSection;

    use super::*;

    fn artifact_with(registrations: Vec<ContractRegistration>) -> LoadedModArtifact {
        LoadedModArtifact {
            discovered: DiscoveredMod {
                dependency_name: "ModA".to_owned(),
                project_name: "ModA".to_owned(),
                project_root: PathBuf::from("/mods/ModA"),
                manifest_path: PathBuf::from("/mods/ModA/Project.proj"),
                source_root: PathBuf::from("/mods/ModA/Src"),
                mod_section: Some(ProjectModSection {
                    max_generator_rounds: None,
                    capabilities: Some(vec![
                        "emit_syntax".to_owned(),
                        "rewrite_syntax".to_owned(),
                        "query_semantic_snapshot".to_owned(),
                    ]),
                    artifact_policy: None,
                }),
            },
            descriptor: Some(ModArtifactDescriptor {
                schema_version: 1,
                package_id: "ModA".to_owned(),
                package_version: None,
                mod_source_hash: "h".to_owned(),
                lock_hash: "l".to_owned(),
                target_triple: "test".to_owned(),
                compiler_version: "v".to_owned(),
                object_file: "mod.o".to_owned(),
                registrations: registrations.clone(),
                artifact_dir: PathBuf::from("/cache"),
            }),
            registrations,
        }
    }

    fn registration(contract: &str, ty: &str, sym: &str) -> ContractRegistration {
        ContractRegistration {
            contract_id: contract.to_owned(),
            type_id: ty.to_owned(),
            entry_symbol: sym.to_owned(),
        }
    }

    #[test]
    fn duplicate_in_artifact_emits_e1829() {
        let loaded = vec![artifact_with(vec![
            registration("Beskid.Compiler.Collect.Generator", "T", "sym1"),
            registration("Beskid.Compiler.Collect.Generator", "T", "sym2"),
        ])];

        let err = validate_registrations(&loaded).unwrap_err();
        assert!(err.codes().contains(&"E1829"));
    }

    #[test]
    fn unknown_contract_id_emits_e1853() {
        let loaded = vec![artifact_with(vec![registration(
            "Beskid.Compiler.Made.Up",
            "T",
            "sym1",
        )])];
        let err = validate_registrations(&loaded).unwrap_err();
        assert!(err.codes().contains(&"E1853"));
    }

    #[test]
    fn rewriter_without_analyzer_emits_e1854() {
        let loaded = vec![artifact_with(vec![registration(
            "Beskid.Compiler.Collect.Rewriter",
            "T",
            "sym1",
        )])];
        let err = validate_registrations(&loaded).unwrap_err();
        assert!(err.codes().contains(&"E1854"));
    }

    #[test]
    fn missing_entry_symbol_emits_e1828() {
        let loaded = vec![artifact_with(vec![registration(
            "Beskid.Compiler.Collect.Generator",
            "T",
            "",
        )])];
        let err = validate_registrations(&loaded).unwrap_err();
        assert!(err.codes().contains(&"E1828"));
    }

    #[test]
    fn cross_artifact_conflict_emits_e1851() {
        let pair = registration("Beskid.Compiler.Collect.Generator", "Shared", "sym");
        let mut a = artifact_with(vec![pair.clone()]);
        a.discovered.project_name = "ModA".into();
        a.descriptor.as_mut().unwrap().package_id = "ModA".into();
        let mut b = artifact_with(vec![ContractRegistration {
            entry_symbol: "sym_b".into(),
            ..pair.clone()
        }]);
        b.discovered.project_name = "ModB".into();
        b.descriptor.as_mut().unwrap().package_id = "ModB".into();

        let err = validate_registrations(&[a, b]).unwrap_err();
        assert!(err.codes().contains(&"E1851"));
    }

    #[test]
    fn cross_artifact_entry_symbol_collision_emits_e1852() {
        let mut a = artifact_with(vec![registration(
            "Beskid.Compiler.Collect.Generator",
            "TypeA",
            "shared_symbol",
        )]);
        a.discovered.project_name = "ModA".into();
        a.descriptor.as_mut().unwrap().package_id = "ModA".into();

        let mut b = artifact_with(vec![registration(
            "Beskid.Compiler.Collect.Analyzer",
            "TypeB",
            "shared_symbol",
        )]);
        b.discovered.project_name = "ModB".into();
        b.descriptor.as_mut().unwrap().package_id = "ModB".into();

        let err = validate_registrations(&[a, b]).unwrap_err();
        assert!(err.codes().contains(&"E1852"));
    }

    #[test]
    fn clean_load_validates_with_no_errors() {
        let loaded = vec![artifact_with(vec![
            registration("Beskid.Compiler.Collect.Collector", "TC", "c"),
            registration("Beskid.Compiler.Collect.Generator", "TG", "g"),
            registration("Beskid.Compiler.Collect.Analyzer", "TA", "a"),
            registration("Beskid.Compiler.Collect.Rewriter", "TR", "r"),
        ])];
        validate_registrations(&loaded).unwrap();
    }
}
