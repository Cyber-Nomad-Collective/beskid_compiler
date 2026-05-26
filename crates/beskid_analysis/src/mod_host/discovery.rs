use std::path::Path;

use anyhow::{Context, Result};

use crate::projects::{CompilePlan, ProjectKind, load_manifest_from_path};

use super::types::DiscoveredMod;

pub(crate) fn discover_mod_dependencies(plan: Option<&CompilePlan>) -> Result<Vec<DiscoveredMod>> {
    let Some(plan) = plan else {
        return Ok(Vec::new());
    };

    let mut mods = Vec::new();
    for dependency in &plan.dependency_projects {
        let manifest = load_manifest_from_path(&dependency.manifest_path).with_context(|| {
            format!(
                "failed to load dependency manifest while discovering compiler mods: {}",
                dependency.manifest_path.display()
            )
        })?;

        if manifest.project.kind != ProjectKind::Mod {
            continue;
        }

        mods.push(DiscoveredMod {
            dependency_name: dependency.dependency_name.clone(),
            project_name: dependency.project_name.clone(),
            project_root: dependency.project_root.clone(),
            manifest_path: dependency.manifest_path.clone(),
            source_root: dependency.source_root.clone(),
            mod_section: manifest.project.mod_section.clone(),
        });
    }

    mods.sort_by(|left, right| {
        stable_path_key(&left.manifest_path)
            .cmp(&stable_path_key(&right.manifest_path))
            .then_with(|| left.project_name.cmp(&right.project_name))
    });
    mods.dedup_by(|left, right| left.manifest_path == right.manifest_path);
    Ok(mods)
}

fn stable_path_key(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::projects::{CompilePlan, ResolvedDependencyProject, Target, TargetKind};

    use super::*;

    #[test]
    fn discovers_only_mod_dependencies_in_stable_order() {
        let root = unique_temp_dir("mod_host_discovery");
        let host = root.join("Host");
        let mod_a = root.join("ModA");
        let lib = root.join("Lib");
        fs::create_dir_all(mod_a.join("Src")).expect("mod dir");
        fs::create_dir_all(lib.join("Src")).expect("lib dir");
        fs::create_dir_all(host.join("Src")).expect("host dir");
        write_manifest(
            &mod_a,
            r#"
project {
  name = "ModA"
  version = "0.1.0"
  type = Mod
  mod {
    capabilities = [emit_syntax]
  }
}
"#,
        );
        write_manifest(
            &lib,
            r#"
project {
  name = "Lib"
  version = "0.1.0"
}

target "lib" {
  kind = Lib
  entry = "Lib.bd"
}
"#,
        );

        let plan = CompilePlan {
            project_root: host.clone(),
            manifest_path: host.join("Project.proj"),
            project_name: "Host".to_owned(),
            source_root: host.join("Src"),
            target: Target {
                name: "main".to_owned(),
                kind: TargetKind::App,
                entry: "Main.bd".to_owned(),
            },
            dependency_projects: vec![
                dependency("Lib", &lib),
                dependency("ModA", &mod_a),
                dependency("ModA", &mod_a),
            ],
            unresolved_dependencies: Vec::new(),
            has_std_dependency: false,
        };

        let discovered = discover_mod_dependencies(Some(&plan)).expect("discover mods");
        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].project_name, "ModA");

        let _ = fs::remove_dir_all(root); // Discard result: temp dir cleanup
    }

    fn dependency(name: &str, root: &std::path::Path) -> ResolvedDependencyProject {
        ResolvedDependencyProject {
            dependency_name: name.to_owned(),
            manifest_path: root.join("Project.proj"),
            project_root: root.to_path_buf(),
            project_name: name.to_owned(),
            source_root: root.join("Src"),
        }
    }

    fn write_manifest(root: &std::path::Path, source: &str) {
        fs::write(root.join("Project.proj"), source).expect("manifest");
    }

    fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}_{id}"))
    }
}
