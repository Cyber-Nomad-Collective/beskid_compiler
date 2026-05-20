//! Stable `qualifiedName`, `displayName`, and `modulePath` for `api.json` rows.

use std::collections::HashMap;

use crate::resolve::items::{ItemInfo, ItemKind};
use crate::resolve::{ModuleGraph, Resolution};

/// Short label for UI (last `::` segment of the resolver name).
pub fn display_name_for_item(item: &ItemInfo) -> String {
    item.name
        .rsplit("::")
        .next()
        .unwrap_or(item.name.as_str())
        .to_string()
}

/// Logical module path segments for a root item (empty for members).
pub fn module_path_for_item(item: &ItemInfo, module_graph: &ModuleGraph) -> Vec<String> {
    if item.parent_id.is_some() {
        return Vec::new();
    }
    for module in module_graph.modules() {
        if module.scope.values().any(|&id| id == item.id) {
            return module.path.clone();
        }
        if module.items.contains(&item.id) {
            return module.path.clone();
        }
    }
    Vec::new()
}

fn join_path_segments(segments: &[String]) -> String {
    segments.join("::")
}

/// Build qualified names for all items in emission order.
pub fn qualified_names_for_items(resolution: &Resolution) -> HashMap<usize, String> {
    let mut out = HashMap::new();
    for item in &resolution.items {
        let qn = if let Some(parent_id) = item.parent_id {
            let parent_qn = out
                .get(&parent_id.0)
                .cloned()
                .unwrap_or_else(|| resolution.items[parent_id.0].name.clone());
            format!(
                "{}::{}",
                parent_qn,
                display_name_for_item(item)
            )
        } else {
            let module_path = module_path_for_item(item, &resolution.module_graph);
            if module_path.is_empty() {
                item.name.clone()
            } else {
                format!(
                    "{}::{}",
                    join_path_segments(&module_path),
                    display_name_for_item(item)
                )
            }
        };
        out.insert(item.id.0, qn);
    }
    out
}

fn type_kind_links_to_item(kind: ItemKind) -> bool {
    matches!(
        kind,
        ItemKind::Type | ItemKind::Enum | ItemKind::Contract
    )
}

/// Lookup keys for resolving `refItemId` and `@ref` targets (`qualifiedName` first).
pub fn type_ref_lookup_index(resolution: &Resolution) -> HashMap<String, usize> {
    let qnames = qualified_names_for_items(resolution);
    let mut idx = HashMap::new();
    for item in &resolution.items {
        if !type_kind_links_to_item(item.kind) {
            continue;
        }
        let id = item.id.0;
        if let Some(qn) = qnames.get(&id) {
            idx.entry(qn.clone()).or_insert(id);
            if let Some((_, tail)) = qn.rsplit_once("::") {
                idx.entry(tail.to_string()).or_insert(id);
            }
            let dotted = qn.replace("::", ".");
            if dotted != *qn {
                idx.entry(dotted).or_insert(id);
            }
        }
        idx.entry(item.name.clone()).or_insert(id);
    }
    idx
}

/// Resolve a type path string to an item id using [`type_ref_lookup_index`].
pub fn lookup_type_ref_id(path: &str, index: &HashMap<String, usize>) -> Option<usize> {
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    if let Some(&id) = index.get(path) {
        return Some(id);
    }
    let dotted = path.replace('.', "::");
    if let Some(&id) = index.get(dotted.as_str()) {
        return Some(id);
    }
    let suffix = format!("::{path}");
    index
        .iter()
        .find(|(key, _)| key.ends_with(&suffix) || key.as_str() == path)
        .map(|(_, id)| *id)
}
