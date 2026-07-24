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
    let descriptor = find_descriptor(workspace_root, &discovered)?.map(|path| read_descriptor(&path)).transpose()?;
    let registrations = descriptor.as_ref().map(|descriptor| descriptor.registrations.clone()).unwrap_or_default();

    Ok(LoadedModArtifact { discovered, descriptor, registrations })
}

fn read_descriptor(path: &Path) -> Result<ModArtifactDescriptor> {
    let json = fs::read_to_string(path)
        .with_context(|| format!("failed to read mod artifact descriptor {}", path.display()))?;
    let mut descriptor: ModArtifactDescriptor = serde_json::from_str(&json)
        .with_context(|| format!("failed to parse mod artifact descriptor {}", path.display()))?;
    descriptor.artifact_dir = path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    Ok(descriptor)
}

fn find_descriptor(workspace_root: Option<&Path>, discovered: &DiscoveredMod) -> Result<Option<PathBuf>> {
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
    Ok(descriptors.into_iter().next())
}

fn collect_descriptors(root: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in
        fs::read_dir(root).with_context(|| format!("failed to inspect mod artifact cache {}", root.display()))?
    {
        let entry = entry.with_context(|| format!("failed to inspect mod artifact cache {}", root.display()))?;
        let path = entry.path();
        let file_type =
            entry.file_type().with_context(|| format!("failed to inspect mod artifact cache {}", path.display()))?;
        if file_type.is_dir() {
            collect_descriptors(&path, out)?;
        } else if file_type.is_file() && path.file_name().and_then(|name| name.to_str()) == Some(MOD_DESCRIPTOR_FILE) {
            out.push(path);
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
    fn missing_descriptor_keeps_empty_registration_list() {
        let root = unique_temp_dir("mod_host_load_empty");
        let mod_dir = root.join("ModA");
        fs::create_dir_all(mod_dir.join("Src")).expect("mod dir");
        let loaded = load_artifacts(Some(&root), vec![discovered("ModA", &mod_dir)], None).expect("load");

        assert_eq!(loaded.len(), 1);
        assert!(loaded[0].descriptor.is_none());
        assert!(loaded[0].registrations.is_empty());

        let _ = fs::remove_dir_all(root); // Discard result: temp dir cleanup
    }

    #[test]
    fn reads_agent_f_descriptor_sidecar() {
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

        let loaded = load_artifacts(Some(&root), vec![discovered("ModA", &mod_dir)], None).expect("load");

        assert_eq!(loaded[0].registrations.len(), 1);
        assert_eq!(loaded[0].registrations[0].entry_symbol, "moda_emit");
        assert_eq!(loaded[0].descriptor.as_ref().unwrap().artifact_dir, descriptor_dir);

        let _ = fs::remove_dir_all(root); // Discard result: temp dir cleanup
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
