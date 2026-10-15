//! Generic disk materialization for `mod.generate` typed outcomes.
//!
//! Generators contribute typed syntax only. A layout splits a generator's admitted items into
//! files and the canonical formatter renders them; no generator-supplied source text is
//! written or reparsed.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::format::{EmitError, format_program};
use crate::projects::CompilePlan;
use crate::syntax::{Program, SpanInfo, Spanned};

use super::types::ProgramItem;

/// Layout manifest authored by a mod package (for example `generate.layout.json`).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GenerateOutputLayout {
    pub schema_version: u32,
    pub files: Vec<GenerateOutputFile>,
}

/// One materialized output file entry.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GenerateOutputFile {
    /// v1: relative path under the declared output root.
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub header: String,
    /// Number of typed items for this file. Required for every v1 file and for every file of a
    /// multi-file v2 layout; a single-file v2 layout may omit it and receives every item.
    #[serde(default)]
    pub item_count: usize,
    /// v2: generated module file stem (writes `{fileName}.g.bd`).
    #[serde(default)]
    pub file_name: String,
    /// v2: Beskid module path (for example `Core.Text.Regex.Generated`).
    #[serde(default)]
    pub module_path: String,
    /// v2: owning package id (for example `corelib_foundation`).
    #[serde(default)]
    pub package_id: String,
}

impl GenerateOutputLayout {
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Item count of each declared file, in layout order, for `total` admitted items.
    fn item_counts(&self, total: usize) -> Result<Vec<usize>, String> {
        if self.schema_version != 1 && self.schema_version != 2 {
            return Err(format!("unsupported generate layout schemaVersion {} (expected 1 or 2)", self.schema_version));
        }
        if self.files.is_empty() {
            return Err("generate layout declares no files".into());
        }
        if self.schema_version == 2 && self.files.len() == 1 && self.files[0].item_count == 0 {
            return Ok(vec![total]);
        }
        if let Some(file) = self.files.iter().find(|file| file.item_count == 0) {
            return Err(format!(
                "generate layout file `{}{}` must declare a nonzero itemCount",
                file.path, file.file_name
            ));
        }
        let declared: usize = self.files.iter().map(|file| file.item_count).sum();
        if declared != total {
            return Err(format!("generate layout expects {declared} typed items, got {total}"));
        }
        Ok(self.files.iter().map(|file| file.item_count).collect())
    }
}

pub fn load_generate_output_layout(path: &Path) -> Result<GenerateOutputLayout, String> {
    let json =
        fs::read_to_string(path).map_err(|err| format!("failed to read generate layout {}: {err}", path.display()))?;
    GenerateOutputLayout::from_json(&json)
        .map_err(|err| format!("failed to parse generate layout {}: {err}", path.display()))
}

/// Resolve `.generated/{modulePathDirs}/{fileName}.g.bd` under a package root.
pub fn resolve_generated_path(package_root: &Path, module_path: &str, file_name: &str) -> PathBuf {
    let segments: Vec<&str> = module_path.split('.').filter(|segment| !segment.is_empty()).collect();
    let dir_segments = if segments.last().copied() == Some(file_name) {
        &segments[..segments.len().saturating_sub(1)]
    } else {
        segments.as_slice()
    };
    let mut out = package_root.join(".generated");
    for segment in dir_segments {
        out.push(segment);
    }
    out.push(format!("{file_name}.g.bd"));
    out
}

/// Resolve a package project root from a compile plan by registry package id / project name.
pub fn resolve_package_root(plan: &CompilePlan, package_id: &str) -> Option<PathBuf> {
    if plan.project_name == package_id {
        return Some(plan.project_root.clone());
    }
    plan.dependency_projects.iter().find(|dep| dep.project_name == package_id).map(|dep| dep.project_root.clone())
}

/// Write a generator's admitted typed items to the files its layout declares.
///
/// v1 files are written under `output_root` at their relative `path`; v2 files are written to
/// `.generated/{modulePath}/{fileName}.g.bd` under the root of the package named by
/// `packageId` (the Mod project itself when absent). Every file is rendered from typed items
/// by the canonical formatter, prefixed by its declared header.
pub fn write_typed_generate_output(
    plan: Option<&CompilePlan>,
    mod_project_root: &Path,
    output_root: &Path,
    items: &[Spanned<ProgramItem>],
    layout: &GenerateOutputLayout,
) -> Result<(), String> {
    let counts = layout.item_counts(items.len())?;
    let mut offset = 0usize;
    for (file, count) in layout.files.iter().zip(counts) {
        let slice = &items[offset..offset + count];
        offset += count;
        let out_path = if layout.schema_version == 1 {
            if file.path.is_empty() {
                return Err("generate layout v1 entry missing path".into());
            }
            output_root.join(&file.path)
        } else {
            let file_name = required(&file.file_name, "fileName")?;
            let module_path = required(&file.module_path, "modulePath")?;
            let package_root = resolve_output_package_root(plan, mod_project_root, file)?;
            resolve_generated_path(&package_root, module_path, file_name)
        };
        let body = format_items(slice).map_err(|err| err.to_string())?;
        write_text_file(&out_path, &file.header, &body)?;
    }
    Ok(())
}

fn required<'a>(value: &'a str, field: &str) -> Result<&'a str, String> {
    if value.is_empty() {
        return Err(format!("generate layout entry missing {field}"));
    }
    Ok(value)
}

fn resolve_output_package_root(
    plan: Option<&CompilePlan>,
    mod_project_root: &Path,
    file: &GenerateOutputFile,
) -> Result<PathBuf, String> {
    if file.package_id.is_empty() {
        return Ok(mod_project_root.to_path_buf());
    }
    if let Some(plan) = plan
        && let Some(root) = resolve_package_root(plan, &file.package_id)
    {
        return Ok(root);
    }
    Err(format!("unable to resolve package root for packageId `{}`", file.package_id))
}

fn write_text_file(out_path: &Path, header: &str, body: &str) -> Result<(), String> {
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    fs::write(out_path, format!("{header}{body}"))
        .map_err(|err| format!("failed to write {}: {err}", out_path.display()))?;
    Ok(())
}

fn format_items(items: &[Spanned<ProgramItem>]) -> Result<String, EmitError> {
    let program =
        Spanned::new(Program { items: items.to_vec(), leading_docs: vec![None; items.len()] }, SpanInfo::default());
    format_program(&program)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mod_host::emit_bridge::materialize_program_items;

    fn unique_dir(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("time").as_nanos()
        ))
    }

    fn file_v2(item_count: usize, file_name: &str) -> GenerateOutputFile {
        GenerateOutputFile {
            path: String::new(),
            header: "// generated\n".into(),
            item_count,
            file_name: file_name.into(),
            module_path: "Core.Text.Regex.Generated".into(),
            package_id: String::new(),
        }
    }

    #[test]
    fn resolve_generated_path_maps_module_path_to_dot_generated() {
        let root = PathBuf::from("/pkg");
        let path = resolve_generated_path(&root, "Core.Text.Regex.Generated", "Generated");
        assert_eq!(path, PathBuf::from("/pkg/.generated/Core/Text/Regex/Generated.g.bd"));
    }

    #[test]
    fn layout_v1_splits_typed_items_into_files() {
        let items = materialize_program_items([
            "pub contract DemoStep { DemoStep Run(); }",
            "pub type DemoFluent { i64 inner }",
            "pub DemoFluent FromDemo() { return DemoFluent { inner: 0 }; }",
        ])
        .expect("materialize");
        let layout = GenerateOutputLayout {
            schema_version: 1,
            files: vec![GenerateOutputFile {
                path: "Demo.bd".into(),
                header: "// test header\n".into(),
                item_count: 3,
                file_name: String::new(),
                module_path: String::new(),
                package_id: String::new(),
            }],
        };
        let output = unique_dir("generate_output_v1");
        write_typed_generate_output(None, &output, &output, &items, &layout).expect("write");
        let written = fs::read_to_string(output.join("Demo.bd")).expect("read");
        assert!(written.starts_with("// test header\n"));
        assert!(written.contains("pub contract DemoStep"));
        let _ = fs::remove_dir_all(output);
    }

    #[test]
    fn layout_v2_renders_typed_items_under_package_generated_root() {
        let package = unique_dir("generate_output_v2");
        let items = materialize_program_items(["pub i64 Demo() { return 1; }"]).expect("materialize");
        let layout = GenerateOutputLayout { schema_version: 2, files: vec![file_v2(0, "Generated")] };
        write_typed_generate_output(None, &package, &package.join("Generated"), &items, &layout).expect("write");
        let written = fs::read_to_string(package.join(".generated/Core/Text/Regex/Generated.g.bd")).expect("read");
        assert!(written.starts_with("// generated\n"));
        assert!(written.contains("pub i64 Demo()"));
        let _ = fs::remove_dir_all(package);
    }

    #[test]
    fn layout_v2_multi_file_requires_exact_item_counts() {
        let package = unique_dir("generate_output_v2_counts");
        let items =
            materialize_program_items(["pub i64 First() { return 1; }", "pub i64 Second() { return 2; }"]).expect("items");
        let missing = GenerateOutputLayout { schema_version: 2, files: vec![file_v2(1, "One"), file_v2(0, "Two")] };
        assert!(write_typed_generate_output(None, &package, &package, &items, &missing).is_err());
        let short = GenerateOutputLayout { schema_version: 2, files: vec![file_v2(1, "One"), file_v2(2, "Two")] };
        assert!(write_typed_generate_output(None, &package, &package, &items, &short).is_err());
        let exact = GenerateOutputLayout { schema_version: 2, files: vec![file_v2(1, "One"), file_v2(1, "Two")] };
        write_typed_generate_output(None, &package, &package, &items, &exact).expect("exact split");
        let second = fs::read_to_string(package.join(".generated/Core/Text/Regex/Generated/Two.g.bd")).expect("read");
        assert!(second.contains("Second") && !second.contains("First"));
        let unknown = GenerateOutputLayout { schema_version: 3, files: vec![file_v2(0, "Generated")] };
        assert!(write_typed_generate_output(None, &package, &package, &items, &unknown).is_err());
        let _ = fs::remove_dir_all(package);
    }
}
