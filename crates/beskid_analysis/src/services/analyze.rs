//! High-level analysis: parse source, run semantic rules, and filter diagnostics using project context.

use std::borrow::ToOwned;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::AnalysisOptions;
use crate::analysis::SemanticDiagnostic;
use crate::compilation_context::CompilationContext;
use crate::projects::CompilePlan;

use super::input::AnalyzeInProjectOptions;
use super::parse::parse_program_with_source_name;
use super::semantic::semantic_rule_diagnostics_for_program;

pub fn analyze_program(path: &Path, source: &str) -> Result<Vec<SemanticDiagnostic>> {
    analyze_program_with_options(path, source, AnalysisOptions::default())
}

pub fn analyze_program_with_options(
    path: &Path,
    source: &str,
    options: AnalysisOptions,
) -> Result<Vec<SemanticDiagnostic>> {
    let program = parse_program_with_source_name(&path.display().to_string(), source)?;
    Ok(semantic_rule_diagnostics_for_program(
        &program.node,
        path.display().to_string(),
        source,
        options,
    ))
}

pub fn analyze_file_in_project(path: &Path) -> Result<Vec<SemanticDiagnostic>> {
    let source = fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;
    analyze_source_in_project(path, &source)
}

pub fn analyze_source_in_project(path: &Path, source: &str) -> Result<Vec<SemanticDiagnostic>> {
    analyze_source_in_project_with_options(path, source, AnalyzeInProjectOptions::default())
}

/// Run [`analyze_source_in_project_with_options`] using a pre-resolved [`CompilationContext`]
/// (for example from an LSP cache) to avoid rebuilding the compile plan.
pub fn analyze_source_with_compilation_context(
    path: &Path,
    source: &str,
    ctx: &CompilationContext,
) -> Result<Vec<SemanticDiagnostic>> {
    let mut rule_options = AnalysisOptions::default();
    rule_options.module_level_meta_items_allowed = Some(ctx.module_level_meta_items_allowed());

    let mut diagnostics = analyze_program_with_options(path, source, rule_options)?;

    if is_non_entry_project_file(path, ctx.compile_plan.as_ref()) {
        diagnostics.retain(|diagnostic| diagnostic.code.as_deref() == Some("parse"));
        return Ok(diagnostics);
    }

    let symbol_hints = collect_symbol_hints_from_source(source, &ctx.module_roots);

    diagnostics.retain(|diagnostic| match diagnostic.code.as_deref() {
        Some("E1105") => {
            if let Some(module_path) = extract_unknown_module_path(&diagnostic.message) {
                return !module_path_exists_on_disk(&module_path, &ctx.module_roots);
            }
            if let Some(import_path) = extract_unknown_import_path(&diagnostic.message) {
                return !module_path_exists_on_disk(&import_path, &ctx.module_roots);
            }
            true
        }
        Some("E1201") => {
            let Some(type_name) = extract_unknown_type_name(&diagnostic.message) else {
                return true;
            };
            !symbol_hints.iter().any(|hint| hint == &type_name)
        }
        Some("E1301") => {
            let Some(enum_root) = extract_unknown_enum_root(&diagnostic.message) else {
                return true;
            };
            !symbol_hints.iter().any(|hint| hint == &enum_root)
        }
        _ => true,
    });

    Ok(diagnostics)
}

pub fn analyze_source_in_project_with_options(
    path: &Path,
    source: &str,
    options: AnalyzeInProjectOptions<'_>,
) -> Result<Vec<SemanticDiagnostic>> {
    let mut graph_opts = options.project_graph.clone();
    if graph_opts.workspace_member_for_meta_default.is_none() {
        if let Some(member) = options.workspace_member {
            graph_opts.workspace_member_for_meta_default = Some(member.to_string());
        }
    }

    match CompilationContext::try_for_analysis_path_with_graph_options(
        path,
        options.workspace_member,
        graph_opts,
    ) {
        Some(ctx) => analyze_source_with_compilation_context(path, source, &ctx),
        None => analyze_program(path, source),
    }
}

pub fn compile_plan_for_input_path(path: &Path) -> Option<CompilePlan> {
    CompilationContext::try_for_analysis_path(path, None).and_then(|c| c.compile_plan)
}

pub fn compile_plan_for_input_path_with_member(
    path: &Path,
    workspace_member: Option<&str>,
) -> Option<CompilePlan> {
    CompilationContext::try_for_analysis_path(path, workspace_member).and_then(|c| c.compile_plan)
}

fn is_non_entry_project_file(path: &Path, plan: Option<&CompilePlan>) -> bool {
    let Some(plan) = plan else {
        return false;
    };
    let entry_path = plan.source_root.join(&plan.target.entry);
    path != entry_path
}

fn collect_symbol_hints_from_source(source: &str, module_roots: &[PathBuf]) -> Vec<String> {
    let mut hints = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("use ") {
            let without_semicolon = rest.trim_end_matches(';').trim();
            let import_path = without_semicolon
                .split_once(" as ")
                .map(|(path, _)| path.trim())
                .unwrap_or(without_semicolon);
            if module_path_exists_on_disk(import_path, module_roots) {
                if let Some(name) = import_path
                    .split('.')
                    .next_back()
                    .map(str::trim)
                    .filter(|part| !part.is_empty())
                {
                    hints.push(name.to_string());
                }
            }
        }

        for token in
            trimmed.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'))
        {
            if token.matches('.').count() < 1 {
                continue;
            }
            let Some((module_path, symbol_name)) = token.rsplit_once('.') else {
                continue;
            };
            if module_path_exists_on_disk(module_path, module_roots) && !symbol_name.is_empty() {
                hints.push(symbol_name.to_string());
            }
        }
    }

    hints.sort();
    hints.dedup();
    hints
}

fn extract_unknown_module_path(message: &str) -> Option<String> {
    message
        .strip_prefix("unknown module path `")
        .and_then(|tail| tail.strip_suffix('`'))
        .map(ToOwned::to_owned)
}

fn extract_unknown_import_path(message: &str) -> Option<String> {
    message
        .strip_prefix("unknown import path `")
        .and_then(|tail| tail.strip_suffix('`'))
        .map(ToOwned::to_owned)
}

fn extract_unknown_type_name(message: &str) -> Option<String> {
    message
        .strip_prefix("unknown type `")
        .and_then(|tail| tail.strip_suffix('`'))
        .map(ToOwned::to_owned)
}

fn extract_unknown_enum_root(message: &str) -> Option<String> {
    let enum_path = message
        .strip_prefix("unknown enum path `")
        .and_then(|tail| tail.strip_suffix('`'))?;
    enum_path.split_once("::").map(|(root, _)| root.to_string())
}

fn module_path_exists_on_disk(module_path: &str, module_roots: &[PathBuf]) -> bool {
    if module_roots.is_empty() {
        return false;
    }

    let normalized = module_path.replace("::", ".");
    let segments: Vec<&str> = normalized
        .split('.')
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments.is_empty() {
        return false;
    }

    let mut relative = PathBuf::new();
    for segment in segments {
        relative.push(segment);
    }

    module_roots.iter().any(|root| {
        let file_candidate = root.join(relative.with_extension("bd"));
        let mod_candidate = root.join(&relative).join("mod.bd");
        file_candidate.is_file() || mod_candidate.is_file()
    })
}
