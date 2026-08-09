use std::collections::HashSet;

use crate::resolve::{ItemKind, Resolution};

use super::contracts::{
    completion_kind_from_item_kind, completion_kind_from_symbol_kind, item_kind_name, symbol_kind_name,
};
use super::model::{CompletionInfo, CompletionKind, DocumentAnalysisSnapshot};
use super::symbols::collect_document_symbols;

fn member_access_prefix(source_text: &str, offset: usize) -> Option<(String, String)> {
    let prefix = source_text.get(..offset)?;
    let mut alias_end = offset;
    let bytes = prefix.as_bytes();
    let mut index = offset;
    while index > 0 {
        index -= 1;
        let ch = bytes[index];
        if ch.is_ascii_alphanumeric() || ch == b'_' {
            alias_end = index;
            continue;
        }
        if ch == b'.' {
            let alias_start = alias_end;
            let alias = prefix.get(alias_start..offset)?.to_string();
            if alias.is_empty() || !alias.as_bytes()[0].is_ascii_alphabetic() && alias.as_bytes()[0] != b'_' {
                return None;
            }
            let partial_start = index + 1;
            let partial = prefix.get(partial_start..offset).unwrap_or("").to_string();
            return Some((alias, partial));
        }
        return None;
    }
    None
}

fn use_path_prefix(source_text: &str, offset: usize) -> Option<String> {
    let line_start = source_text[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line = source_text.get(line_start..offset)?;
    let trimmed = line.trim_start();
    if !trimmed.starts_with("use ") {
        return None;
    }
    let path_part = trimmed.strip_prefix("use ")?.trim_start();
    if path_part.contains(';') {
        return None;
    }
    Some(path_part.to_string())
}

fn module_path_display(path: &[String]) -> String {
    path.join("::")
}

fn member_completion_candidates(resolution: &Resolution, alias: &str, partial: &str) -> Vec<CompletionInfo> {
    let Some(module_path) = resolution.module_imports.get(alias) else {
        return Vec::new();
    };
    let Some(module_id) = resolution.module_graph.module_id(module_path) else {
        return Vec::new();
    };
    let Some(module) = resolution.module_graph.module(module_id) else {
        return Vec::new();
    };
    let module_label = module_path_display(module_path);
    let partial_lower = partial.to_lowercase();

    module
        .scope
        .keys()
        .filter(|name| partial.is_empty() || name.to_lowercase().starts_with(partial_lower.as_str()))
        .filter_map(|name| {
            let item_id = module.scope.get(name)?;
            let item = resolution.items.get(item_id.0)?;
            if !matches!(item.kind, ItemKind::Function | ItemKind::Method | ItemKind::Type | ItemKind::Enum) {
                return None;
            }
            Some(CompletionInfo {
                label: name.clone(),
                kind: completion_kind_from_item_kind(item.kind),
                detail: Some(module_label.clone()),
            })
        })
        .collect()
}

fn use_path_completion_candidates(
    typed_prefix: &str,
    assembly_module_paths: &HashSet<String>,
    module_graph: &crate::resolve::ModuleGraph,
) -> Vec<CompletionInfo> {
    let typed = typed_prefix.trim();
    let typed_segments: Vec<&str> = typed.split('.').filter(|s| !s.is_empty()).collect();
    let partial = if typed.ends_with('.') { "" } else { typed_segments.last().copied().unwrap_or("") };
    let parent_path: Vec<&str> = if typed.ends_with('.') {
        typed_segments
    } else {
        typed_segments[..typed_segments.len().saturating_sub(1)].to_vec()
    };
    let partial_lower = partial.to_lowercase();

    let paths: Vec<String> = if !assembly_module_paths.is_empty() {
        assembly_module_paths.iter().cloned().collect()
    } else {
        module_graph
            .modules()
            .iter()
            .filter(|module| !module.path.is_empty())
            .map(|module| module_path_display(&module.path))
            .collect()
    };

    let mut candidates = Vec::new();
    for path in paths {
        let segments: Vec<&str> = path.split("::").collect();
        if segments.len() <= parent_path.len() {
            continue;
        }
        if parent_path.iter().zip(segments.iter()).any(|(left, right)| *left != *right) {
            continue;
        }
        let Some(next) = segments.get(parent_path.len()) else {
            continue;
        };
        if !partial.is_empty() && !next.to_lowercase().starts_with(partial_lower.as_str()) {
            continue;
        }
        // Completion inserts the next segment at the cursor.  Its display label must therefore
        // not repeat the already-typed module prefix (for example, `use Std.` offers `Core`,
        // not `Std.Core`).
        let label = (*next).to_string();
        candidates.push(CompletionInfo { label: label.clone(), kind: CompletionKind::Module, detail: Some(path) });
    }

    candidates.sort_by(|left, right| left.label.cmp(&right.label));
    candidates.dedup_by(|left, right| left.label == right.label);
    candidates
}

pub fn completion_candidates(
    snapshot: &DocumentAnalysisSnapshot,
    source_text: &str,
    offset: usize,
) -> Vec<CompletionInfo> {
    if let Some((alias, partial)) = member_access_prefix(source_text, offset)
        && let Some(resolution) = snapshot.resolution.as_ref()
    {
        let members = member_completion_candidates(resolution, &alias, &partial);
        if !members.is_empty() {
            return members;
        }
    }

    if let Some(use_prefix) = use_path_prefix(source_text, offset)
        && let Some(resolution) = snapshot.resolution.as_ref()
    {
        let paths =
            use_path_completion_candidates(&use_prefix, &snapshot.assembly_module_paths, &resolution.module_graph);
        if !paths.is_empty() {
            return paths;
        }
    }

    let Some(resolution) = snapshot.resolution.as_ref() else {
        return collect_document_symbols(snapshot)
            .into_iter()
            .map(|symbol| CompletionInfo {
                label: symbol.name,
                kind: completion_kind_from_symbol_kind(symbol.kind),
                detail: Some(symbol_kind_name(symbol.kind).to_string()),
            })
            .collect();
    };

    let mut candidates = Vec::new();
    for item in &resolution.items {
        candidates.push(CompletionInfo {
            label: item.name.clone(),
            kind: completion_kind_from_item_kind(item.kind),
            detail: Some(item_kind_name(item.kind).to_string()),
        });
    }
    for local in &resolution.tables.locals {
        candidates.push(CompletionInfo {
            label: local.name.clone(),
            kind: CompletionKind::Variable,
            detail: Some("local".to_string()),
        });
    }

    candidates.sort_by(|left, right| left.label.cmp(&right.label));
    candidates.dedup_by(|left, right| left.label == right.label && left.kind == right.kind);
    candidates
}
