//! Stable `qualifiedName`, `displayName`, and `modulePath` for `api.json` rows.

use std::collections::HashMap;

use crate::resolve::items::{ItemInfo, ItemKind};
use crate::resolve::symbol::symbol_key;
use crate::resolve::symbol_lookup::qualified_name;
use crate::resolve::{ModuleGraph, Resolution};

/// Short label for UI (last `::` segment of the resolver name).
pub fn display_name_for_item(item: &ItemInfo) -> String {
    item.name.rsplit("::").next().unwrap_or(item.name.as_str()).to_string()
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

fn legacy_qualified_name(item: &ItemInfo, resolution: &Resolution, cache: &HashMap<usize, String>) -> String {
    if let Some(parent_id) = item.parent_id {
        let parent_qn = cache.get(&parent_id.0).cloned().unwrap_or_else(|| resolution.items[parent_id.0].name.clone());
        return format!("{}::{}", parent_qn, display_name_for_item(item));
    }
    let module_path = module_path_for_item(item, &resolution.module_graph);
    if module_path.is_empty() {
        item.name.clone()
    } else {
        format!("{}::{}", join_path_segments(&module_path), display_name_for_item(item))
    }
}

/// Build qualified names for all items in emission order.
pub fn qualified_names_for_items(resolution: &Resolution) -> HashMap<usize, String> {
    let mut out = HashMap::new();
    for item in &resolution.items {
        let qn = qualified_name(resolution, item.id).unwrap_or_else(|| legacy_qualified_name(item, resolution, &out));
        out.insert(item.id.0, qn);
    }
    out
}

fn type_kind_links_to_item(kind: ItemKind) -> bool {
    matches!(kind, ItemKind::Type | ItemKind::Enum | ItemKind::Contract)
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
        if let Some(symbol) = item.symbol
            && let Some(key) = symbol_key(&resolution.symbols, symbol)
        {
            idx.entry(key).or_insert(id);
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
    index.iter().find(|(key, _)| key.ends_with(&suffix) || key.as_str() == path).map(|(_, id)| *id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::HirVisibility;
    use std::collections::HashMap;

    use crate::resolve::{
        ExportKind, ItemId, ItemInfo, ItemKind, ModuleGraph, Resolution, ResolutionTables, SymbolQualifier,
        SymbolRegistry, SymbolShape,
    };
    use crate::syntax::SpanInfo;

    #[test]
    fn type_ref_lookup_index_includes_registry_symbol_key() {
        let mut registry = SymbolRegistry::default();
        let symbol = registry.intern(SymbolQualifier {
            package: "demo".into(),
            shape: SymbolShape::ModuleItem {
                module_path: vec!["Root".into()],
                name: "Widget".into(),
                kind: ExportKind::Type,
            },
        });
        let item_id = ItemId(0);
        let resolution = Resolution {
            items: vec![ItemInfo {
                id: item_id,
                parent_id: None,
                name: "Widget".into(),
                kind: ItemKind::Type,
                visibility: HirVisibility::Public,
                span: SpanInfo::from_byte_range_in_source("", 0, 1),
                source_path: None,
                symbol: Some(symbol),
            }],
            module_graph: ModuleGraph::new_root(),
            tables: ResolutionTables::new(),
            span_index: Default::default(),
            warnings: vec![],
            builtin_items: HashMap::new(),
            module_imports: HashMap::new(),
            symbols: registry,
            by_symbol: HashMap::from([(symbol, item_id)]),
        };
        let index = type_ref_lookup_index(&resolution);
        assert_eq!(
            lookup_type_ref_id("demo::Root::Widget", &index),
            Some(0),
            "symbolKey must be indexed for @ref / refItemId lookup"
        );
    }
}
