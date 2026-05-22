//! Copy and transform template source trees per manifest `sources` rules.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use walkdir::WalkDir;

use crate::error::{TemplateError, TemplateResult};
use crate::guids::{replace_guids_in_text, verify_guids_replaced};
use crate::manifest::{TemplateManifest, TEMPLATE_MANIFEST_REL};
use crate::substitute::{
    apply_source_name, ensure_no_placeholders_remain, substitute_path_component, substitute_text,
};

#[derive(Debug, Clone)]
pub struct SourceWritePlan {
    pub relative_output: PathBuf,
    pub bytes: Vec<u8>,
    pub is_binary: bool,
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

            let rel = path
                .strip_prefix(&source_root)
                .map_err(|_| TemplateError::Internal("strip_prefix failed".into()))?;
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            if rel_str.ends_with(TEMPLATE_MANIFEST_REL) {
                continue;
            }

            if !include.is_match(&rel_str) || exclude.is_match(&rel_str) {
                continue;
            }

            let mut target_rel =
                substitute_path_component(&block.target.trim_start_matches("./"), values);
            if target_rel == "." || target_rel.is_empty() {
                target_rel = String::new();
            }
            let mut out_rel = PathBuf::from(&target_rel).join(rel);
            if let Some(rename) = block.rename.get(&rel_str) {
                out_rel = PathBuf::from(substitute_path_component(rename, values));
            }
            out_rel = output_root.join(out_rel);

            let rel_for_plan = out_rel
                .strip_prefix(output_root)
                .unwrap_or(&out_rel)
                .to_path_buf();

            let bytes = fs::read(path)?;
            let process_text = !copy_only.is_match(&rel_str) && is_probably_text(&bytes);
            let content = if process_text {
                let mut text = String::from_utf8_lossy(&bytes).into_owned();
                if let Some(source_name) = &manifest.source_name {
                    if let Some(primary) = values.get(manifest.primary_name_symbol_id()) {
                        text = apply_source_name(&text, source_name, primary);
                    }
                }
                text = substitute_text(&text, values);
                text = replace_guids_in_text(&text, &manifest.guids, guids_map)?;
                ensure_no_placeholders_remain(&text)?;
                verify_guids_replaced(&text, &manifest.guids)?;
                text.into_bytes()
            } else {
                bytes
            };

            plans.push(SourceWritePlan {
                relative_output: rel_for_plan,
                bytes: content,
                is_binary: !process_text,
            });
        }
    }

    Ok(plans)
}

pub fn apply_write_plans(
    output_root: &Path,
    plans: &[SourceWritePlan],
    force: bool,
) -> TemplateResult<()> {
    for plan in plans {
        let dest = output_root.join(&plan.relative_output);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        if dest.exists() && !force {
            return Err(TemplateError::OutputConflict { path: dest });
        }
        fs::write(&dest, &plan.bytes)?;
    }
    Ok(())
}

fn build_glob_set(patterns: &[String]) -> TemplateResult<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|e| {
            TemplateError::InvalidManifest(format!("invalid glob `{pattern}`: {e}"))
        })?;
        builder.add(glob);
    }
    Ok(builder.build().map_err(|e| TemplateError::Internal(e.to_string()))?)
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
                return Err(TemplateError::InvalidManifest(
                    "output path cannot contain `..`".to_string(),
                ));
            }
            Component::Normal(part) => normalized.push(part),
            Component::RootDir | Component::Prefix(_) => normalized.push(component.as_os_str()),
        }
    }
    Ok(normalized)
}
