//! Copy and transform template source trees per manifest `sources` rules.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use walkdir::WalkDir;

use crate::error::{TemplateError, TemplateResult};
use crate::guids::{replace_guids_in_text, scan_leftover_guid_patterns, verify_guids_replaced};
use crate::manifest::{TEMPLATE_MANIFEST_REL, TemplateManifest};
use crate::substitute::{apply_source_name, ensure_no_placeholders_remain, substitute_path_component, substitute_text};

#[derive(Debug, Clone)]
pub struct SourceWritePlan {
    pub relative_output: PathBuf,
    pub bytes: Vec<u8>,
}

pub fn plan_source_writes(
    template_root: &Path,
    manifest: &TemplateManifest,
    output_root: &Path,
    values: &BTreeMap<String, String>,
    guids_map: &mut std::collections::HashMap<String, String>,
) -> TemplateResult<Vec<SourceWritePlan>> {
    let mut plans = Vec::new();

    for block in &manifest.sources {
        if !block.condition {
            continue;
        }
        let source_root = template_root.join(block.source.trim_start_matches("./"));
        if !source_root.exists() {
            return Err(TemplateError::InvalidManifest(format!(
                "source path `{}` does not exist",
                source_root.display()
            )));
        }

        let include = build_glob_set(&block.include)?;
        let exclude = build_glob_set(&block.exclude)?;
        let copy_only = build_glob_set(&block.copy_only)?;

        for entry in WalkDir::new(&source_root).into_iter().filter_map(Result::ok) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let rel =
                path.strip_prefix(&source_root).map_err(|_| TemplateError::Internal("strip_prefix failed".into()))?;
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            if rel_str.ends_with(TEMPLATE_MANIFEST_REL) {
                continue;
            }

            if !include.is_match(&rel_str) || exclude.is_match(&rel_str) {
                continue;
            }

            let target_rel = substitute_relative_output_path(&block.target, values)?;
            let out_rel = if let Some(rename) = block.rename.get(&rel_str) {
                substitute_relative_output_path(rename, values)?
            } else {
                target_rel.join(substitute_relative_output_path(&rel_str, values)?)
            };
            let rel_for_plan = normalize_relative_output_path(&out_rel)?;
            validate_output_destination(output_root, &rel_for_plan)?;

            let bytes = fs::read(path)?;
            let process_text = !copy_only.is_match(&rel_str) && is_probably_text(&bytes);
            let content = if process_text {
                let mut text = String::from_utf8_lossy(&bytes).into_owned();
                if let Some(source_name) = &manifest.source_name
                    && let Some(primary) = values.get(manifest.primary_name_symbol_id())
                {
                    text = apply_source_name(&text, source_name, primary);
                }
                text = substitute_text(&text, values);
                text = replace_guids_in_text(&text, &manifest.guids, guids_map)?;
                ensure_no_placeholders_remain(&text)?;
                verify_guids_replaced(&text, &manifest.guids)?;
                scan_leftover_guid_patterns(&text, &manifest.guids)?;
                text.into_bytes()
            } else {
                bytes
            };

            plans.push(SourceWritePlan { relative_output: rel_for_plan, bytes: content });
        }
    }

    Ok(plans)
}

pub fn apply_write_plans(output_root: &Path, plans: &[SourceWritePlan], force: bool) -> TemplateResult<()> {
    let mut destinations = BTreeSet::new();
    for plan in plans {
        let relative_output = normalize_relative_output_path(&plan.relative_output)?;
        if !destinations.insert(relative_output.clone()) {
            return Err(TemplateError::InvalidManifest(format!(
                "multiple template sources produce output path `{}`",
                relative_output.display()
            )));
        }
        validate_output_destination(output_root, &relative_output)?;
        let dest = output_root.join(relative_output);
        if dest.exists() && !force {
            return Err(TemplateError::OutputConflict { path: dest });
        }
    }

    for plan in plans {
        let dest = output_root.join(normalize_relative_output_path(&plan.relative_output)?);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&dest, &plan.bytes)?;
    }
    Ok(())
}

fn substitute_relative_output_path(path: &str, values: &BTreeMap<String, String>) -> TemplateResult<PathBuf> {
    let portable_path = path.replace('\\', "/");
    let mut substituted = PathBuf::new();
    for component in Path::new(&portable_path).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(TemplateError::InvalidManifest("output path cannot contain `..`".to_string()));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(TemplateError::InvalidManifest("output path must be relative".to_string()));
            }
            Component::Normal(part) => {
                let part = substitute_path_component(&part.to_string_lossy(), values);
                ensure_no_placeholders_remain(&part)?;
                let portable_part = part.replace('\\', "/");
                for substituted_component in Path::new(&portable_part).components() {
                    match substituted_component {
                        Component::CurDir => {}
                        Component::Normal(part) => substituted.push(part),
                        Component::ParentDir => {
                            return Err(TemplateError::InvalidManifest("output path cannot contain `..`".to_string()));
                        }
                        Component::RootDir | Component::Prefix(_) => {
                            return Err(TemplateError::InvalidManifest("output path must be relative".to_string()));
                        }
                    }
                }
            }
        }
    }
    normalize_relative_output_path(&substituted)
}

fn normalize_relative_output_path(path: &Path) -> TemplateResult<PathBuf> {
    let portable_path = path.to_string_lossy().replace('\\', "/");
    let mut normalized = PathBuf::new();
    for component in Path::new(&portable_path).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(TemplateError::InvalidManifest("output path cannot contain `..`".to_string()));
            }
            Component::Normal(part) => normalized.push(part),
            Component::RootDir | Component::Prefix(_) => {
                return Err(TemplateError::InvalidManifest("output path must be relative".to_string()));
            }
        }
    }
    Ok(normalized)
}

fn validate_output_destination(output_root: &Path, relative_output: &Path) -> TemplateResult<()> {
    let output_root = fs::canonicalize(output_root)?;
    let relative_output = normalize_relative_output_path(relative_output)?;
    let components = relative_output.components().collect::<Vec<_>>();
    let mut current = output_root.clone();
    for (index, component) in components.iter().enumerate() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(_) => {
                let resolved = fs::canonicalize(&current)?;
                if !resolved.starts_with(&output_root) {
                    return Err(TemplateError::InvalidManifest(format!(
                        "output path `{}` resolves outside the output root",
                        relative_output.display()
                    )));
                }
                if index + 1 < components.len() && !resolved.is_dir() {
                    return Err(TemplateError::InvalidManifest(format!(
                        "output path parent `{}` is not a directory",
                        current.display()
                    )));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn build_glob_set(patterns: &[String]) -> TemplateResult<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob =
            Glob::new(pattern).map_err(|e| TemplateError::InvalidManifest(format!("invalid glob `{pattern}`: {e}")))?;
        builder.add(glob);
    }
    builder.build().map_err(|e| TemplateError::Internal(e.to_string()))
}

fn is_probably_text(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return true;
    }
    if bytes.contains(&0) {
        return false;
    }
    std::str::from_utf8(bytes).is_ok()
}

pub fn normalize_output_path(path: &Path) -> TemplateResult<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(TemplateError::InvalidManifest("output path cannot contain `..`".to_string()));
            }
            Component::Normal(part) => normalized.push(part),
            Component::RootDir | Component::Prefix(_) => normalized.push(component.as_os_str()),
        }
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::parse_manifest_bytes;
    use std::collections::BTreeMap;

    struct Fixture {
        root: PathBuf,
        template_root: PathBuf,
        output_root: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("beskid-template-sources-{}", uuid::Uuid::new_v4()));
            let template_root = root.join("template");
            let output_root = root.join("output");
            fs::create_dir_all(&template_root).unwrap();
            fs::create_dir_all(&output_root).unwrap();
            Self { root, template_root, output_root }
        }

        fn write_source(&self, relative: &str, bytes: &[u8]) {
            let path = self.template_root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn manifest(target: &str) -> TemplateManifest {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "schema": "beskid.template.v1",
            "identity": "test.template",
            "name": "Test",
            "shortName": "test",
            "sources": [{
                "source": "./",
                "target": target,
                "include": ["**/*"]
            }]
        }))
        .unwrap();
        parse_manifest_bytes(&bytes).unwrap()
    }

    fn plan(fixture: &Fixture, target: &str, values: &[(&str, &str)]) -> TemplateResult<Vec<SourceWritePlan>> {
        let values = values.iter().map(|(key, value)| (key.to_string(), value.to_string())).collect();
        plan_source_writes(
            &fixture.template_root,
            &manifest(target),
            &fixture.output_root,
            &values,
            &mut Default::default(),
        )
    }

    #[test]
    fn substitutes_tokens_in_source_filename() {
        let fixture = Fixture::new();
        fixture.write_source("content/{{name}}.bproj", b"project");

        let plans = plan(&fixture, "./", &[("name", "MyApp")]).unwrap();

        assert_eq!(plans[0].relative_output, PathBuf::from("content/MyApp.bproj"));
    }

    #[test]
    fn substitutes_tokens_in_nested_workspace_paths() {
        let fixture = Fixture::new();
        fixture.write_source("{{workspaceName}}/src/{{name}}.bd", b"source");

        let plans = plan(&fixture, "./", &[("workspaceName", "MyWorkspace"), ("name", "MyApp")]).unwrap();

        assert_eq!(plans[0].relative_output, PathBuf::from("MyWorkspace/src/MyApp.bd"));
    }

    #[test]
    fn unresolved_filename_tokens_fail_before_any_write() {
        let fixture = Fixture::new();
        fixture.write_source("content/{{missing}}.bproj", b"project");

        let result = plan(&fixture, "./", &[]).and_then(|plans| apply_write_plans(&fixture.output_root, &plans, false));

        assert!(matches!(result, Err(TemplateError::InvalidManifest(_))));
        assert_eq!(fs::read_dir(&fixture.output_root).unwrap().count(), 0);
    }

    #[test]
    fn substituted_filename_collisions_fail_before_any_write() {
        let fixture = Fixture::new();
        fixture.write_source("{{name}}.bd", b"generated");
        fixture.write_source("MyApp.bd", b"existing");

        let result = plan(&fixture, "./", &[("name", "MyApp")])
            .and_then(|plans| apply_write_plans(&fixture.output_root, &plans, false));

        assert!(matches!(result, Err(TemplateError::InvalidManifest(_))));
        assert_eq!(fs::read_dir(&fixture.output_root).unwrap().count(), 0);
    }

    #[test]
    fn traversal_and_absolute_output_paths_fail_before_any_write() {
        let fixture = Fixture::new();
        fixture.write_source("source.bd", b"source");

        let traversal = plan(&fixture, "../escape", &[]);
        assert!(matches!(traversal, Err(TemplateError::InvalidManifest(_))));

        let mut absolute_manifest = manifest("./");
        absolute_manifest.sources[0].rename.insert("source.bd".to_string(), "/tmp/escape.bd".to_string());
        let absolute = plan_source_writes(
            &fixture.template_root,
            &absolute_manifest,
            &fixture.output_root,
            &BTreeMap::new(),
            &mut Default::default(),
        );
        assert!(matches!(absolute, Err(TemplateError::InvalidManifest(_))));
        assert_eq!(fs::read_dir(&fixture.output_root).unwrap().count(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn output_paths_cannot_redirect_through_symlinks() {
        let fixture = Fixture::new();
        let outside = fixture.root.join("outside");
        fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, fixture.output_root.join("redirect")).unwrap();
        fixture.write_source("source.bd", b"source");

        let result = plan(&fixture, "redirect", &[]);

        assert!(matches!(result, Err(TemplateError::InvalidManifest(_))));
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
    }
}
