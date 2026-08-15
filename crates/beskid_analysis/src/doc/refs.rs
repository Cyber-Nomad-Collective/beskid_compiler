//! Shared `@ref(...)` resolution for doc rendering and validation.

use std::path::PathBuf;

use crate::resolve::symbol::symbol_key;
use crate::resolve::{ItemInfo, Resolution};

use super::qualified_names::{lookup_type_ref_id, qualified_names_for_items, type_ref_lookup_index};

/// Registry documentation route context for turning resolved `@ref` paths into markdown links.
///
/// `package_with_version` is the raw `{{package}}@{{version}}` segment used by pckg
/// (`/docs/{{PackageWithVersion}}/api/...`), before per-segment percent-encoding.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct DocRefLinkContext {
    pub package_with_version: String,
    /// Publishing package id (same package as `package_with_version` prefix).
    pub publishing_package: Option<String>,
    /// Dependency source roots → registry package id for cross-package doc links.
    pub dependency_roots: Vec<(PathBuf, String)>,
}

fn percent_encode_path_segment(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for &b in input.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(char::from(b));
            }
            _ => {
                use std::fmt::Write as _;
                let _ = write!(out, "%{b:02X}"); // Discard result: writing to String is infallible
            }
        }
    }
    out
}

fn escape_markdown_link_text(label: &str) -> String {
    label.replace('\\', "\\\\").replace('[', "\\[").replace(']', "\\]")
}

fn version_suffix(package_with_version: &str) -> String {
    package_with_version.rsplit_once('@').map(|(_, ver)| ver.to_string()).unwrap_or_else(|| "latest".to_string())
}

fn package_for_item(target: &ItemInfo, ctx: &DocRefLinkContext) -> String {
    let ver = version_suffix(&ctx.package_with_version);
    if let Some(path) = &target.source_path {
        let mut best: Option<(usize, String)> = None;
        for (root, package) in &ctx.dependency_roots {
            if path.starts_with(root) {
                let len = root.as_os_str().len();
                if best.as_ref().is_none_or(|(l, _)| len > *l) {
                    best = Some((len, package.clone()));
                }
            }
        }
        if let Some((_, dep_pkg)) = best
            && ctx.publishing_package.as_ref().is_none_or(|pub_pkg| pub_pkg != &dep_pkg)
        {
            return format!("{dep_pkg}@{ver}");
        }
    }
    ctx.package_with_version.trim().to_string()
}

fn find_resolved_item_id(path: &str, resolution: &Resolution) -> Option<usize> {
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    for item in &resolution.items {
        if let Some(symbol) = item.symbol
            && let Some(key) = symbol_key(&resolution.symbols, symbol)
            && key == path
        {
            return Some(item.id.0);
        }
    }
    let qnames = qualified_names_for_items(resolution);
    if qnames.values().any(|qn| qn == path) {
        return qnames.iter().find(|(_, qn)| *qn == path).map(|(id, _)| *id);
    }
    let index = type_ref_lookup_index(resolution);
    if let Some(id) = lookup_type_ref_id(path, &index) {
        return Some(id);
    }
    for item in &resolution.items {
        if item.name == path {
            return Some(item.id.0);
        }
    }
    let suffix = format!("::{path}");
    for item in &resolution.items {
        if let Some(qn) = qnames.get(&item.id.0)
            && (qn == path || qn.ends_with(&suffix))
        {
            return Some(item.id.0);
        }
    }
    None
}

fn qualified_name_for_id(id: usize, resolution: &Resolution) -> String {
    let qnames = qualified_names_for_items(resolution);
    qnames.get(&id).cloned().unwrap_or_else(|| resolution.items.get(id).map(|i| i.name.clone()).unwrap_or_default())
}

/// Resolve a `@ref` path to a Markdown fragment (markdown link when [DocRefLinkContext] is set, else backticks).
pub fn resolve_ref_markdown(path: &str, resolution: &Resolution, links: Option<&DocRefLinkContext>) -> String {
    let path = path.trim();
    if path.is_empty() {
        return "`@ref()`".to_string();
    }
    let Some(target_id) = find_resolved_item_id(path, resolution) else {
        return format!("`{path}` _(unresolved)_");
    };
    let Some(target) = resolution.items.get(target_id) else {
        return format!("`{path}` _(unresolved)_");
    };
    let qn = qualified_name_for_id(target_id, resolution);
    if let Some(ctx) = links
        && !ctx.package_with_version.trim().is_empty()
    {
        let pkg = percent_encode_path_segment(&package_for_item(target, ctx));
        let encoded_qn = percent_encode_path_segment(&qn);
        let label = escape_markdown_link_text(&qn);
        return format!("[{label}](/docs/{pkg}/api/{encoded_qn})");
    }
    format!("`{qn}`")
}

/// `true` when the path resolves to a known item (no "unresolved" marker).
pub fn ref_path_resolves(path: &str, resolution: &Resolution) -> bool {
    find_resolved_item_id(path, resolution).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::Visibility;
    use std::collections::HashMap;

    use crate::resolve::{
        ExportKind, ItemId, ItemInfo, ItemKind, ModuleGraph, Resolution, ResolutionTables, SymbolQualifier,
        SymbolRegistry, SymbolShape,
    };
    use crate::syntax::SpanInfo;

    fn item(id: usize, name: &str, kind: ItemKind, parent_id: Option<ItemId>) -> ItemInfo {
        ItemInfo {
            id: ItemId(id),
            parent_id,
            name: name.to_string(),
            kind,
            visibility: Visibility::Public,
            span: SpanInfo::from_byte_range_in_source("", 0, 1),
            source_path: None,
            symbol: None,
        }
    }

    fn sample_resolution() -> Resolution {
        Resolution {
            items: vec![
                item(0, "Widget", ItemKind::Type, None),
                item(1, "value", ItemKind::Field, Some(ItemId(0))),
                item(2, "main", ItemKind::Function, None),
            ],
            module_graph: ModuleGraph::new_root(),
            tables: ResolutionTables::new(),
            span_index: Default::default(),
            warnings: Vec::new(),
            builtin_items: HashMap::new(),
            module_imports: HashMap::new(),
            symbols: Default::default(),
            by_symbol: HashMap::new(),
        }
    }

    #[test]
    fn ref_markdown_link_uses_qualified_name() {
        let resolution = sample_resolution();
        let ctx = DocRefLinkContext {
            package_with_version: "demo@1.0.0".into(),
            publishing_package: Some("demo".into()),
            dependency_roots: vec![],
        };
        let md = resolve_ref_markdown("Widget::value", &resolution, Some(&ctx));
        assert!(md.contains("[Widget::value](/docs/demo%401.0.0/api/Widget%3A%3Avalue)"), "{md}");
    }

    #[test]
    fn ref_markdown_backtick_without_context() {
        let resolution = sample_resolution();
        let md = resolve_ref_markdown("main", &resolution, None);
        assert_eq!(md, "`main`");
    }

    #[test]
    fn ref_resolves_by_registry_symbol_key() {
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
                visibility: Visibility::Public,
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
        assert!(ref_path_resolves("demo::Root::Widget", &resolution));
        let md = resolve_ref_markdown("demo::Root::Widget", &resolution, None);
        assert_eq!(md, "`demo::Root::Widget`");
    }

    #[test]
    fn ref_markdown_unresolved_marker() {
        let resolution = sample_resolution();
        let md = resolve_ref_markdown("Ghost", &resolution, None);
        assert!(md.contains("unresolved"), "{md}");
    }
}
