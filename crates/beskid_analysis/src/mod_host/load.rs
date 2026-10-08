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
        anyhow::bail!("{}", missing_artifact_message(&discovered));
    }
    let registrations = descriptor.as_ref().map(|descriptor| descriptor.registrations.clone()).unwrap_or_default();

    Ok(LoadedModArtifact { discovered, descriptor, registrations })
}

/// Actionable explanation for a missing executable descriptor: where it is expected and how to
/// produce it. Toolchain-provisioned Corelib Mods ship it; any other Mod is built by the CLI.
fn missing_artifact_message(discovered: &DiscoveredMod) -> String {
    let expected = discovered
        .project_root
        .join(".beskid")
        .join("obj")
        .join("mods")
        .join(&discovered.project_name);
    let shipped = beskid_abi::corelib_bundle::verified_corelib_bundle_root(&discovered.manifest_path).is_some();
    let remedy = if shipped {
        "this Mod belongs to the toolchain Corelib bundle, which ships its prebuilt executable descriptor; \
         reinstall the toolchain, or build it with"
    } else {
        "build it with"
    };
    format!(
        "required native Mod artifact is missing for dependency {} ({}): no current executable descriptor under {}; \
         {remedy} `beskid dev mod rebuild {}` (any `beskid build`, `run` or `check` of this project also builds it)",
        discovered.dependency_name,
        discovered.project_name,
        expected.display(),
        discovered.manifest_path.display()
    )
}

fn source_is_current(descriptor: &ModArtifactDescriptor, discovered: &DiscoveredMod) -> Result<bool> {
    let plan = crate::projects::build_compile_plan(&discovered.manifest_path, None)
        .map_err(|error| anyhow::anyhow!("resolve native Mod dependency closure: {error}"))?;
    let dependency_sources = super::descriptor::native_mod_dependency_sources(&plan)?;
    super::descriptor::native_mod_evidence_is_current(
        descriptor,
        &discovered.project_root,
        &discovered.manifest_path,
        &discovered.source_root,
        &dependency_sources,
    )
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
    let profile = super::descriptor::native_mod_runtime_profile()?;
    let mut selected = None;
    let mut rejected = Vec::new();
    for path in descriptors {
        // Each runtime-kit profile has its own cache entry; another profile's entry is not a candidate.
        if descriptor_profile(&path).is_some_and(|candidate| candidate != profile) {
            continue;
        }
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
            "no current qualified native Mod artifact for {}; rebuild it with `beskid dev mod rebuild {}`: {}",
            discovered.project_name,
            discovered.manifest_path.display(),
            rejected.join("; ")
        );
    }
    Ok(selected)
}

/// The runtime profile a descriptor records, read without validating the artifact.
fn descriptor_profile(path: &Path) -> Option<beskid_abi::runtime_kit::BuildProfile> {
    let bytes = fs::read(path).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    serde_json::from_value(value.get("runtime")?.get("profile")?.clone()).ok()
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
        let message = error.to_string();
        assert!(message.contains("required native Mod artifact is missing"), "{message}");
        assert!(message.contains("beskid dev mod rebuild"), "missing descriptor names the remedy: {message}");
        assert!(message.contains(".beskid"), "missing descriptor names the expected location: {message}");

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

    fn evidence_is_current(descriptor: &ModArtifactDescriptor, discovered: &DiscoveredMod) -> bool {
        evidence_is_current_with(descriptor, discovered, &std::collections::BTreeMap::new())
    }

    fn evidence_is_current_with(
        descriptor: &ModArtifactDescriptor,
        discovered: &DiscoveredMod,
        dependencies: &std::collections::BTreeMap<String, String>,
    ) -> bool {
        super::super::descriptor::native_mod_evidence_is_current(
            descriptor,
            &discovered.project_root,
            &discovered.manifest_path,
            &discovered.source_root,
            dependencies,
        )
        .unwrap()
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
        assert!(evidence_is_current(&descriptor, &discovered));
        fs::write(root.join("Src/Added.bd"), "pub type Added {}").unwrap();
        assert!(!evidence_is_current(&descriptor, &discovered), "new source membership invalidates native authority");
        fs::remove_file(root.join("Src/Added.bd")).unwrap();
        fs::write(root.join(crate::projects::PROJECT_LOCK_FILE_NAME), "new lock").unwrap();
        assert!(!evidence_is_current(&descriptor, &discovered), "a newly present lock cannot be ignored");
        fs::remove_file(root.join(crate::projects::PROJECT_LOCK_FILE_NAME)).unwrap();
        fs::write(&manifest, "changed manifest").unwrap();
        assert!(!evidence_is_current(&descriptor, &discovered));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dependency_source_change_makes_cached_evidence_stale() {
        let root = unique_temp_dir("mod-dependency-evidence");
        let dependency = root.join("Dep");
        fs::create_dir_all(dependency.join("src/nested")).unwrap();
        fs::create_dir_all(dependency.join("src/obj/beskid")).unwrap();
        let dependency_manifest = dependency.join("Dep.bproj");
        fs::write(&dependency_manifest, "Dep { name = \"Dep\" version = \"0.1.0\" }").unwrap();
        fs::write(dependency.join("src/Lib.bd"), "pub unit Helper() { return; }").unwrap();
        let identity = || {
            super::super::descriptor::native_mod_dependency_identity(&dependency_manifest, &dependency.join("src"))
                .unwrap()
        };
        let original = identity();
        fs::write(dependency.join("src/obj/beskid/Generated.bd"), "build output").unwrap();
        assert_eq!(identity(), original, "build output is outside the dependency package");

        let mod_root = root.join("ModA");
        fs::create_dir_all(mod_root.join("Src")).unwrap();
        fs::write(mod_root.join("Project.proj"), "manifest").unwrap();
        fs::write(mod_root.join("Src/Main.bd"), "pub unit Main() { return; }").unwrap();
        let discovered = discovered("ModA", &mod_root);
        let mut descriptor = ModArtifactDescriptor::context_fixture();
        for relative in ["Project.proj", "Src/Main.bd"] {
            descriptor.source_files.insert(
                format!("sources/{relative}"),
                super::super::descriptor::native_mod_file_sha256(&mod_root.join(relative)).unwrap(),
            );
        }
        descriptor.dependency_sources.insert("Dep".to_owned(), original.clone());
        let recorded = std::collections::BTreeMap::from([("Dep".to_owned(), original.clone())]);
        assert!(evidence_is_current_with(&descriptor, &discovered, &recorded));

        fs::write(dependency.join("src/nested/Added.bd"), "pub type Added {}").unwrap();
        let changed = std::collections::BTreeMap::from([("Dep".to_owned(), identity())]);
        assert_ne!(changed, recorded);
        assert!(!evidence_is_current_with(&descriptor, &discovered, &changed), "changed dependency source is stale");
        assert!(
            !evidence_is_current_with(&descriptor, &discovered, &std::collections::BTreeMap::new()),
            "a removed dependency is stale"
        );
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
