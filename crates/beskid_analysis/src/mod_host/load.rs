use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use beskid_pipeline::{PipelineObserver, observe_phase_result, phases::MOD_LOAD};

use super::types::{DiscoveredMod, LoadedModArtifact, ModArtifactDescriptor};

const MOD_DESCRIPTOR_FILE: &str = "mod.descriptor.json";

pub(crate) fn load_artifacts(
    workspace_root: Option<&Path>,
    mods: Vec<DiscoveredMod>,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<Vec<LoadedModArtifact>> {
    observe_phase_result(pipeline, MOD_LOAD, || {
        mods.into_iter().map(|discovered| load_artifact(workspace_root, discovered)).collect()
    })
}

fn load_artifact(workspace_root: Option<&Path>, discovered: DiscoveredMod) -> Result<LoadedModArtifact> {
    let descriptor = find_descriptor(workspace_root, &discovered)?;
    if descriptor.is_none() {
        anyhow::bail!(
            "required native Mod artifact is missing for dependency {} ({}); build its executable descriptor",
            discovered.dependency_name,
            discovered.project_name
        );
    }
    let registrations = descriptor.as_ref().map(|descriptor| descriptor.registrations.clone()).unwrap_or_default();

    Ok(LoadedModArtifact { discovered, descriptor, registrations })
}

fn source_is_current(descriptor: &ModArtifactDescriptor, discovered: &DiscoveredMod) -> Result<bool> {
    for (name, digest) in &descriptor.source_files {
        let relative = name.strip_prefix("sources/").context("Mod source evidence outside source closure")?;
        let current = discovered.project_root.join(relative);
        if !current.exists() || super::descriptor::native_mod_file_sha256(&current)? != *digest {
            return Ok(false);
        }
    }
    let source_inventory = super::descriptor::mod_artifact_inventory(&discovered.source_root)?;
    let source_relative = discovered
        .source_root
        .strip_prefix(&discovered.project_root)?
        .to_str()
        .context("non UTF8 Mod source root")?
        .replace(std::path::MAIN_SEPARATOR, "/");
    for (name, digest) in source_inventory {
        if descriptor.source_files.get(&format!("sources/{source_relative}/{name}")) != Some(&digest) {
            return Ok(false);
        }
    }
    for path in [
        discovered.manifest_path.clone(),
        discovered.project_root.join(crate::projects::PROJECT_LOCK_FILE_NAME),
        discovered.project_root.join("project.mod"),
    ] {
        if path.exists() {
            let relative = path
                .strip_prefix(&discovered.project_root)?
                .to_str()
                .context("non UTF8 Mod authority path")?
                .replace(std::path::MAIN_SEPARATOR, "/");
            if descriptor.source_files.get(&format!("sources/{relative}"))
                != Some(&super::descriptor::native_mod_file_sha256(&path)?)
            {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn read_descriptor(path: &Path) -> Result<ModArtifactDescriptor> {
    let prefix = beskid_abi::runtime_kit::installed_runtime_prefix()
        .map_err(|error| anyhow::anyhow!("native Mod runtime prefix: {error}"))?;
    super::descriptor::read_mod_artifact_descriptor(path, &prefix)
}

fn find_descriptor(workspace_root: Option<&Path>, discovered: &DiscoveredMod) -> Result<Option<ModArtifactDescriptor>> {
    let mut roots = BTreeSet::new();
    if let Some(workspace_root) = workspace_root {
        roots.insert(workspace_root.to_path_buf());
    }
    roots.insert(discovered.project_root.clone());
    if let Some(source_parent) = discovered.source_root.parent() {
        roots.insert(source_parent.to_path_buf());
    }

    let package_candidates = [discovered.project_name.as_str(), discovered.dependency_name.as_str()];
    let mut descriptors = Vec::new();

    for root in roots {
        for package_id in package_candidates {
            let artifact_root = root.join(".beskid").join("obj").join("mods").join(package_id);
            if artifact_root.is_dir() {
                collect_descriptors(&artifact_root, &mut descriptors)?;
            }
        }
    }

    descriptors.sort();
    descriptors.dedup();
    let mut selected = None;
    let mut rejected = Vec::new();
    for path in descriptors {
        match read_descriptor(&path).and_then(|descriptor| {
            if !source_is_current(&descriptor, discovered)? {
                anyhow::bail!("stale source, manifest or lock closure");
            }
            Ok(descriptor)
        }) {
            Ok(descriptor) => {
                if selected.replace(descriptor).is_some() {
                    anyhow::bail!(
                        "ambiguous qualified native Mod cache for {}; rebuild after removing conflicting candidates",
                        discovered.project_name
                    );
                }
            }
            Err(error) => {
                if rejected.len() < 8 {
                    rejected.push(format!("{}: {error}", path.display()));
                }
            }
        }
    }
    if selected.is_none() && !rejected.is_empty() {
        anyhow::bail!(
            "no current qualified native Mod artifact for {}; rebuild the Mod: {}",
            discovered.project_name,
            rejected.join("; ")
        );
    }
    Ok(selected)
}

fn collect_descriptors(root: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    collect_descriptors_bounded(root, out, 0)
}
fn collect_descriptors_bounded(root: &Path, out: &mut Vec<PathBuf>, depth: usize) -> Result<()> {
    if depth > 64 || out.len() > 4096 {
        anyhow::bail!("native Mod cache inventory exceeds bounds");
    }
    for entry in
        fs::read_dir(root).with_context(|| format!("failed to inspect mod artifact cache {}", root.display()))?
    {
        let entry = entry.with_context(|| format!("failed to inspect mod artifact cache {}", root.display()))?;
        let path = entry.path();
        let file_type =
            entry.file_type().with_context(|| format!("failed to inspect mod artifact cache {}", path.display()))?;
        if file_type.is_dir() {
            collect_descriptors_bounded(&path, out, depth + 1)?;
        } else if file_type.is_file() && path.file_name().and_then(|name| name.to_str()) == Some(MOD_DESCRIPTOR_FILE) {
            out.push(path.canonicalize()?);
        } else if file_type.is_symlink() {
            anyhow::bail!("native Mod cache symlink is forbidden: {}", path.display());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::projects::ProjectModSection;

    use super::*;

    #[test]
    fn missing_descriptor_rejects_before_registration() {
        let root = unique_temp_dir("mod_host_load_empty");
        let mod_dir = root.join("ModA");
        fs::create_dir_all(mod_dir.join("Src")).expect("mod dir");
        let error = load_artifacts(Some(&root), vec![discovered("ModA", &mod_dir)], None).unwrap_err();
        assert!(error.to_string().contains("required native Mod artifact is missing"));

        let _ = fs::remove_dir_all(root); // Discard result: temp dir cleanup
    }

    #[test]
    fn legacy_descriptor_never_qualifies_an_executable() {
        let root = unique_temp_dir("mod_host_load_descriptor");
        let mod_dir = root.join("ModA");
        let descriptor_dir = root.join(".beskid/obj/mods/ModA/cache-key/aarch64-apple-darwin");
        fs::create_dir_all(mod_dir.join("Src")).expect("mod dir");
        fs::create_dir_all(&descriptor_dir).expect("descriptor dir");
        fs::write(
            descriptor_dir.join(MOD_DESCRIPTOR_FILE),
            r#"{
  "schemaVersion": 1,
  "packageId": "ModA",
  "modSourceHash": "source",
  "lockHash": "lock",
  "targetTriple": "aarch64-apple-darwin",
  "compilerVersion": "test",
  "objectFile": "mod.o",
  "registrations": [
    {
      "contractId": "Beskid.Compiler.Collect.Generator",
      "typeId": "ModA.Emit",
      "entrySymbol": "moda_emit"
    }
  ]
}"#,
        )
        .expect("descriptor");

        let error = load_artifacts(Some(&root), vec![discovered("ModA", &mod_dir)], None).unwrap_err();
        assert!(error.to_string().contains("no current qualified native Mod artifact"));

        let _ = fs::remove_dir_all(root); // Discard result: temp dir cleanup
    }

    #[test]
    fn current_source_evidence_rejects_stale_added_source_and_new_lock() {
        let root = unique_temp_dir("mod-current-source-evidence");
        fs::create_dir_all(root.join("Src")).unwrap();
        let manifest = root.join("Project.proj");
        fs::write(&manifest, "manifest").unwrap();
        fs::write(root.join("Src/Main.bd"), "pub unit Main() { return; }").unwrap();
        let discovered = discovered("ModA", &root);
        let mut descriptor = ModArtifactDescriptor::context_fixture();
        for (relative, path) in [("Project.proj", manifest.clone()), ("Src/Main.bd", root.join("Src/Main.bd"))] {
            descriptor.source_files.insert(
                format!("sources/{relative}"),
                super::super::descriptor::native_mod_file_sha256(&path).unwrap(),
            );
        }
        assert!(source_is_current(&descriptor, &discovered).unwrap());
        fs::write(root.join("Src/Added.bd"), "pub type Added {}").unwrap();
        assert!(
            !source_is_current(&descriptor, &discovered).unwrap(),
            "new source membership invalidates native authority"
        );
        fs::remove_file(root.join("Src/Added.bd")).unwrap();
        fs::write(root.join(crate::projects::PROJECT_LOCK_FILE_NAME), "new lock").unwrap();
        assert!(!source_is_current(&descriptor, &discovered).unwrap(), "a newly present lock cannot be ignored");
        fs::remove_file(root.join(crate::projects::PROJECT_LOCK_FILE_NAME)).unwrap();
        fs::write(&manifest, "changed manifest").unwrap();
        assert!(!source_is_current(&descriptor, &discovered).unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    fn discovered(name: &str, root: &Path) -> DiscoveredMod {
        DiscoveredMod {
            dependency_name: name.to_owned(),
            project_name: name.to_owned(),
            project_root: root.to_path_buf(),
            manifest_path: root.join("Project.proj"),
            source_root: root.join("Src"),
            mod_section: Some(ProjectModSection {
                max_generator_rounds: None,
                capabilities: None,
                artifact_policy: None,
                generated_outputs: None,
            }),
        }
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let id = SystemTime::now().duration_since(UNIX_EPOCH).expect("time").as_nanos();
        std::env::temp_dir().join(format!("{prefix}_{id}"))
    }
}
