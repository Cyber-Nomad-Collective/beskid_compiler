use std::path::PathBuf;

use beskid_analysis::hir::{HirCallExpression, HirExpressionNode};
use beskid_analysis::paths::same_file;
use beskid_analysis::resolve::{ItemId, ItemKind, Resolution, ResolvedValue, canonical_item_id};
use beskid_analysis::syntax::Spanned;

pub fn resolve_path_item_id(resolution: &Resolution, segments: &[String]) -> Option<ItemId> {
    item_id_from_module_graph(resolution, segments).map(|item_id| canonical_item_id(resolution, item_id))
}
pub(crate) fn resolve_item_call_id(
    call: &Spanned<HirCallExpression>,
    resolution: &Resolution,
    source_path: Option<&PathBuf>,
) -> Option<ItemId> {
    if let Some(segments) = path_segments_from_call(call)
        && let Some(item_id) = item_id_from_module_graph(resolution, &segments)
    {
        return Some(canonical_item_id(resolution, item_id));
    }

    let callee_span = match &call.node.callee.node {
        HirExpressionNode::PathExpression(path) => path.node.path.span,
        _ => call.node.callee.span,
    };
    let item_id =
        if let Some(ResolvedValue::Item(item_id)) = resolution.tables.resolved_value_at(callee_span, source_path) {
            item_id
        } else {
            item_id_for_call_path(resolution, call, source_path)?
        };
    Some(canonical_item_id(resolution, item_id))
}

fn path_segments_from_call(call: &Spanned<HirCallExpression>) -> Option<Vec<String>> {
    callee_path_segments(&call.node.callee)
}

fn callee_path_segments(callee: &Spanned<HirExpressionNode>) -> Option<Vec<String>> {
    match &callee.node {
        HirExpressionNode::PathExpression(path) => {
            Some(path.node.path.node.segments.iter().map(|segment| segment.node.name.node.name.clone()).collect())
        }
        HirExpressionNode::MemberExpression(member) => {
            let mut segments = callee_path_segments(&member.node.target)?;
            segments.push(member.node.member.node.name.clone());
            Some(segments)
        }
        HirExpressionNode::GroupedExpression(grouped) => callee_path_segments(&grouped.node.expr),
        _ => None,
    }
}

fn item_id_from_module_graph(resolution: &Resolution, segments: &[String]) -> Option<ItemId> {
    if segments.is_empty() {
        return None;
    }
    let name = segments.last()?;
    for module_path in candidate_module_paths(resolution, segments) {
        let Some(module_id) = resolution.module_graph.module_id(&module_path) else {
            continue;
        };
        let Some(module) = resolution.module_graph.module(module_id) else {
            continue;
        };
        if let Some(item_id) = module.scope.get(name) {
            return Some(*item_id);
        }
    }
    None
}

fn candidate_module_paths(resolution: &Resolution, segments: &[String]) -> Vec<Vec<String>> {
    if segments.len() < 2 {
        return Vec::new();
    }
    let prefix = &segments[..segments.len() - 1];
    let mut paths = vec![prefix.to_vec()];
    if let Some(import_target) = resolution.module_imports.get(&segments[0]) {
        let mut expanded = import_target.clone();
        expanded.extend_from_slice(&segments[1..segments.len() - 1]);
        paths.push(expanded);
    }
    if prefix.first().map(String::as_str) != Some("Platform") {
        let mut with_platform = vec!["Platform".to_string()];
        with_platform.extend_from_slice(prefix);
        paths.push(with_platform);
    }
    paths
}

fn item_id_for_call_path(
    resolution: &Resolution,
    call: &Spanned<HirCallExpression>,
    source_path: Option<&PathBuf>,
) -> Option<ItemId> {
    let segments = path_segments_from_call(call)?;
    if segments.len() == 1
        && let Some(path) = source_path
    {
        for info in &resolution.items {
            if !matches!(info.kind, ItemKind::Function | ItemKind::Method) {
                continue;
            }
            let display = info.name.rsplit("::").next().unwrap_or(info.name.as_str());
            if display != segments[0].as_str() {
                continue;
            }
            if info.source_path.as_ref().is_some_and(|source| same_file(source, path)) {
                return Some(info.id);
            }
        }
    }

    let name = segments.last()?;
    let module_suffix = if segments.len() > 1 { segments[..segments.len() - 1].join("::") } else { String::new() };
    let mut matches = Vec::new();
    for &item_id in resolution.by_symbol.values() {
        let Some(info) = resolution.items.get(item_id.0) else {
            continue;
        };
        if !matches!(info.kind, ItemKind::Function | ItemKind::Method) {
            continue;
        }
        let display = info.name.rsplit("::").next().unwrap_or(info.name.as_str());
        if display != name.as_str() {
            continue;
        }
        let Some(qn) = beskid_analysis::resolve::qualified_name(resolution, item_id) else {
            continue;
        };
        if !module_suffix.is_empty()
            && !qn.contains(&module_suffix)
            && !info.name.contains(&format!("::{module_suffix}::"))
        {
            continue;
        }
        matches.push(item_id);
    }
    match matches.as_slice() {
        [] => None,
        [single] => Some(*single),
        many => {
            if let Some(path) = source_path
                && let Some(item) = many.iter().find(|item| {
                    resolution.items.get(item.0).is_some_and(|info| {
                        info.source_path.as_ref().is_some_and(|source| beskid_analysis::paths::same_file(source, path))
                    })
                })
            {
                return Some(*item);
            }
            many.last().copied()
        }
    }
}
