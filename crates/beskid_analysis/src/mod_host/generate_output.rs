//! Generic disk materialization for `mod.generate` typed and code outcomes.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::format::{EmitError, format_program};
use crate::projects::CompilePlan;
use crate::syntax::{Program, SpanInfo, Spanned};

use super::types::ProgramItem;

const DEFAULT_SINGLE_FILE: &str = "generated.g.bd";

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
    /// v1: relative path under output root (deprecated).
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub header: String,
    /// v1: number of typed AST items for this file.
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

    pub fn expected_item_count(&self) -> usize {
        if self.schema_version >= 2 {
            return 0;
        }
        self.files.iter().map(|file| file.item_count).sum()
    }
}

pub fn load_generate_output_layout(path: &Path) -> Result<GenerateOutputLayout, String> {
    let json = fs::read_to_string(path)
        .map_err(|err| format!("failed to read generate layout {}: {err}", path.display()))?;
    GenerateOutputLayout::from_json(&json)
        .map_err(|err| format!("failed to parse generate layout {}: {err}", path.display()))
}

/// Resolve `.generated/{modulePathDirs}/{fileName}.g.bd` under a package root.
pub fn resolve_generated_path(package_root: &Path, module_path: &str, file_name: &str) -> PathBuf {
    let segments: Vec<&str> = module_path
        .split('.')
        .filter(|segment| !segment.is_empty())
        .collect();
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
    plan.dependency_projects
        .iter()
        .find(|dep| dep.project_name == package_id)
        .map(|dep| dep.project_root.clone())
}

/// Write mod generator typed items to disk using an optional layout manifest.
pub fn write_typed_generate_output(
    output_root: &Path,
    items: &[Spanned<ProgramItem>],
    layout: Option<&GenerateOutputLayout>,
) -> Result<(), String> {
    match layout {
        Some(layout) if layout.schema_version >= 2 => {
            write_code_outputs_v2(output_root, layout, items)
        }
        Some(layout) => write_with_layout_v1(output_root, items, layout),
        None => write_single_file(output_root, DEFAULT_SINGLE_FILE, "", items),
    }
}

/// Write evaluated Beskid source text to `.generated/` paths declared in a v2 layout.
pub fn write_code_generate_output(
    plan: Option<&CompilePlan>,
    mod_project_root: &Path,
    layout: &GenerateOutputLayout,
    outputs: &[CodeGenerateOutput],
) -> Result<(), String> {
    if layout.schema_version < 2 {
        return Err("write_code_generate_output requires layout schemaVersion >= 2".into());
    }
    if layout.files.len() != outputs.len() {
        return Err(format!(
            "generate layout declares {} files but {} code outputs were provided",
            layout.files.len(),
            outputs.len()
        ));
    }
    for (file, output) in layout.files.iter().zip(outputs.iter()) {
        let file_name = required_file_name(file)?;
        let module_path = required_module_path(file, output)?;
        let package_root = resolve_output_package_root(plan, mod_project_root, file)?;
        let out_path = resolve_generated_path(&package_root, &module_path, &file_name);
        write_text_file(&out_path, &file.header, &output.body)?;
    }
    Ok(())
}

/// One evaluated code body destined for disk materialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeGenerateOutput {
    pub module_path: String,
    pub body: String,
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
    Err(format!(
        "unable to resolve package root for packageId `{}`",
        file.package_id
    ))
}

fn required_file_name(file: &GenerateOutputFile) -> Result<String, String> {
    if file.file_name.is_empty() {
        return Err("generate layout entry missing fileName".into());
    }
    Ok(file.file_name.clone())
}

fn required_module_path(
    file: &GenerateOutputFile,
    output: &CodeGenerateOutput,
) -> Result<String, String> {
    if !file.module_path.is_empty() {
        return Ok(file.module_path.clone());
    }
    if output.module_path.is_empty() {
        return Err("generate layout entry missing modulePath".into());
    }
    Ok(output.module_path.clone())
}

fn write_code_outputs_v2(
    output_root: &Path,
    layout: &GenerateOutputLayout,
    items: &[Spanned<ProgramItem>],
) -> Result<(), String> {
    let _ = output_root;
    let _ = layout;
    if !items.is_empty() {
        return Err(
            "schemaVersion 2 layouts require code outputs; typed item materialization is unsupported"
                .into(),
        );
    }
    Ok(())
}

fn write_with_layout_v1(
    output_root: &Path,
    items: &[Spanned<ProgramItem>],
    layout: &GenerateOutputLayout,
) -> Result<(), String> {
    if layout.schema_version != 1 {
        return Err(format!(
            "unsupported generate layout schemaVersion {} (expected 1 or 2)",
            layout.schema_version
        ));
    }
    let expected = layout.expected_item_count();
    if items.len() != expected {
        return Err(format!(
            "generate layout expects {expected} typed items, got {}",
            items.len()
        ));
    }

    let mut offset = 0usize;
    for file in &layout.files {
        let slice = &items[offset..offset + file.item_count];
        offset += file.item_count;
        write_single_file(output_root, &file.path, &file.header, slice)?;
    }
    Ok(())
}

fn write_single_file(
    output_root: &Path,
    relative_path: &str,
    header: &str,
    items: &[Spanned<ProgramItem>],
) -> Result<(), String> {
    let out_path = output_root.join(relative_path);
    let body = format_items(items).map_err(|err| err.to_string())?;
    write_text_file(&out_path, header, &body)
}

fn write_text_file(out_path: &Path, header: &str, body: &str) -> Result<(), String> {
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    fs::write(out_path, format!("{header}{body}"))
        .map_err(|err| format!("failed to write {}: {err}", out_path.display()))?;
    Ok(())
}

fn format_items(items: &[Spanned<ProgramItem>]) -> Result<String, EmitError> {
    let program = Spanned::new(
        Program {
            items: items.to_vec(),
            leading_docs: vec![None; items.len()],
        },
        SpanInfo::default(),
    );
    format_program(&program)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mod_host::emit_bridge::materialize_program_items;

    #[test]
    fn resolve_generated_path_maps_module_path_to_dot_generated() {
        let root = PathBuf::from("/pkg");
        let path = resolve_generated_path(&root, "Core.Text.Regex.Generated", "Generated");
        assert_eq!(
            path,
            PathBuf::from("/pkg/.generated/Core/Text/Regex/Generated.g.bd")
        );
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
        let output = std::env::temp_dir().join(format!(
            "generate_output_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        write_with_layout_v1(&output, &items, &layout).expect("write");
        let written = fs::read_to_string(output.join("Demo.bd")).expect("read");
        assert!(written.starts_with("// test header\n"));
        assert!(written.contains("pub contract DemoStep"));
        let _ = fs::remove_dir_all(output);
    }

    #[test]
    fn write_code_generate_output_writes_g_bd_under_package_root() {
        let package = std::env::temp_dir().join(format!(
            "code_generate_output_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&package).expect("mkdir");
        let layout = GenerateOutputLayout {
            schema_version: 2,
            files: vec![GenerateOutputFile {
                path: String::new(),
                header: "// generated\n".into(),
                item_count: 0,
                file_name: "Generated".into(),
                module_path: "Core.Text.Regex.Generated".into(),
                package_id: String::new(),
            }],
        };
        write_code_generate_output(
            None,
            &package,
            &layout,
            &[CodeGenerateOutput {
                module_path: String::new(),
                body: "pub i64 Demo() { return 1; }".into(),
            }],
        )
        .expect("write");
        let written = fs::read_to_string(package.join(".generated/Core/Text/Regex/Generated.g.bd"))
            .expect("read");
        assert!(written.starts_with("// generated\n"));
        assert!(written.contains("pub i64 Demo()"));
        let _ = fs::remove_dir_all(package);
    }
}
