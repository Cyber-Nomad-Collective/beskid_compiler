//! Shared `@ref(...)` resolution for doc rendering and validation.

use crate::resolve::{ItemInfo, Resolution};

/// Registry documentation route context for turning resolved `@ref` paths into markdown links.
///
/// `package_with_version` is the raw `{{package}}@{{version}}` segment used by pckg
/// (`/docs/{{PackageWithVersion}}/api/...`), before per-segment percent-encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocRefLinkContext {
    pub package_with_version: String,
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
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

fn escape_markdown_link_text(label: &str) -> String {
    label
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

fn find_resolved_item<'a>(path: &str, resolution: &'a Resolution) -> Option<&'a ItemInfo> {
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    for item in &resolution.items {
        if item.name == path {
            return Some(item);
        }
    }
    let suffix = format!("::{path}");
    for item in &resolution.items {
        if item.name.ends_with(&suffix) {
            return Some(item);
        }
    }
    let needle = path.rsplit('.').next().unwrap_or(path);
    for item in &resolution.items {
        if item.name == needle {
            return Some(item);
        }
    }
    for item in &resolution.items {
        if item.name.ends_with(&format!("::{needle}")) {
            return Some(item);
        }
    }
    None
}

/// Resolve a `@ref` path to a Markdown fragment (markdown link when [DocRefLinkContext] is set, else backticks).
pub fn resolve_ref_markdown(
    path: &str,
    resolution: &Resolution,
    links: Option<&DocRefLinkContext>,
) -> String {
    let path = path.trim();
    if path.is_empty() {
        return "`@ref()`".to_string();
    }
    if let Some(target) = find_resolved_item(path, resolution) {
        if let Some(ctx) = links
            && !ctx.package_with_version.trim().is_empty() {
                let pkg = percent_encode_path_segment(ctx.package_with_version.trim());
                let qn = percent_encode_path_segment(&target.name);
                let label = escape_markdown_link_text(&target.name);
                return format!("[{label}](/docs/{pkg}/api/{qn})");
            }
        return format!("`{}`", target.name);
    }
    format!("`{path}` _(unresolved)_")
}

/// `true` when the path resolves to a known item (no "unresolved" marker).
pub fn ref_path_resolves(path: &str, resolution: &Resolution) -> bool {
    find_resolved_item(path, resolution).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::HirVisibility;
    use std::collections::HashMap;

    use crate::resolve::{ItemId, ItemInfo, ItemKind, ModuleGraph, Resolution, ResolutionTables};
    use crate::syntax::SpanInfo;

    fn item(id: usize, name: &str, kind: ItemKind, parent_id: Option<ItemId>) -> ItemInfo {
        ItemInfo {
            id: ItemId(id),
            parent_id,
            name: name.to_string(),
            kind,
            visibility: HirVisibility::Public,
            span: SpanInfo::from_byte_range_in_source("", 0, 1),
        }
    }

    fn sample_resolution() -> Resolution {
        Resolution {
            items: vec![
                item(0, "Widget", ItemKind::Type, None),
                item(1, "Widget::value", ItemKind::Field, Some(ItemId(0))),
                item(2, "main", ItemKind::Function, None),
            ],
            module_graph: ModuleGraph::new_root(),
            tables: ResolutionTables::new(),
            warnings: Vec::new(),
            builtin_items: HashMap::new(),
        }
    }

    #[test]
    fn ref_markdown_link_when_context_and_resolved() {
        let resolution = sample_resolution();
        let ctx = DocRefLinkContext {
            package_with_version: "demo@1.0.0".into(),
        };
        let md = resolve_ref_markdown("Widget::value", &resolution, Some(&ctx));
        assert!(
            md.contains("[Widget::value](/docs/demo%401.0.0/api/Widget%3A%3Avalue)"),
            "{md}"
        );
    }

    #[test]
    fn ref_markdown_backtick_without_context() {
        let resolution = sample_resolution();
        let md = resolve_ref_markdown("main", &resolution, None);
        assert_eq!(md, "`main`");
    }

    #[test]
    fn ref_markdown_unresolved_marker() {
        let resolution = sample_resolution();
        let md = resolve_ref_markdown("Ghost", &resolution, None);
        assert!(md.contains("unresolved"), "{md}");
    }
}
