//! Shared `@ref(...)` resolution for doc rendering and validation.

use crate::resolve::Resolution;

/// Resolve a `@ref` path to a Markdown fragment (backticked name, or unresolved marker).
pub fn resolve_ref_markdown(path: &str, resolution: &Resolution) -> String {
    let path = path.trim();
    if path.is_empty() {
        return "`@ref()`".to_string();
    }
    for item in &resolution.items {
        if item.name == path {
            return format!("`{}`", item.name);
        }
    }
    let suffix = format!("::{path}");
    for item in &resolution.items {
        if item.name.ends_with(&suffix) {
            return format!("`{}`", item.name);
        }
    }
    let needle = path.rsplit('.').next().unwrap_or(path);
    for item in &resolution.items {
        if item.name == needle {
            return format!("`{}`", item.name);
        }
        if item.name.ends_with(&format!("::{needle}")) {
            return format!("`{}`", item.name);
        }
    }
    format!("`{path}` _(unresolved)_")
}

/// `true` when the path resolves to a known item (no "unresolved" marker).
pub fn ref_path_resolves(path: &str, resolution: &Resolution) -> bool {
    let path = path.trim();
    if path.is_empty() {
        return false;
    }
    resolution.items.iter().any(|item| item.name == path)
        || resolution
            .items
            .iter()
            .any(|item| item.name.ends_with(&format!("::{path}")))
        || {
            let needle = path.rsplit('.').next().unwrap_or(path);
            resolution
                .items
                .iter()
                .any(|item| item.name == needle || item.name.ends_with(&format!("::{needle}")))
        }
}
