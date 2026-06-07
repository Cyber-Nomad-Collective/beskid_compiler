//! Merge resolved foreign library inputs into the `link { ... }` block of `Project.proj`.
//!
//! Preserves existing manifest formatting and comments: when a `link` block is already present we
//! rewrite only that block's `libraries`, `searchPaths`, and `extraArgs` fields with the merged
//! union; otherwise we append a fresh block at the end of the file.

use std::path::PathBuf;

use crate::projects::parse_bsol_document;
use crate::projects::model::ProjectLinkSection;

use super::resolution::LibraryResolution;

/// Result of a non-destructive link-block merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkMergeOutcome {
    /// Updated manifest text ready to be written to disk.
    pub updated_source: String,
    /// Manifest contents after merge (parsed shape used for diagnostics / pretty-printing).
    pub merged_section: ProjectLinkSection,
    /// True when a link block already existed (the merge updated it in place).
    pub had_existing_block: bool,
    /// Logical names that were newly added by this merge.
    pub added_libraries: Vec<String>,
    /// Search paths newly added by this merge.
    pub added_search_paths: Vec<PathBuf>,
}

/// Merge a single resolved library into the manifest source text.
pub fn merge_resolution_into_manifest_source(
    manifest_source: &str,
    existing_link: Option<&ProjectLinkSection>,
    resolution: &LibraryResolution,
) -> LinkMergeOutcome {
    let base = existing_link.cloned().unwrap_or_default();
    let mut merged = base.clone();

    let mut added_libraries = Vec::new();
    if !merged.libraries.iter().any(|n| n == &resolution.logical) {
        merged.libraries.push(resolution.logical.clone());
        added_libraries.push(resolution.logical.clone());
    }

    let mut added_search_paths = Vec::new();
    for path in &resolution.search_paths {
        let as_str = path.to_string_lossy().to_string();
        if !merged.search_paths.iter().any(|p| p == &as_str) {
            merged.search_paths.push(as_str.clone());
            added_search_paths.push(path.clone());
        }
    }

    let updated_source = render_manifest_with_link(manifest_source, &merged);
    LinkMergeOutcome {
        updated_source,
        merged_section: merged,
        had_existing_block: existing_link.is_some(),
        added_libraries,
        added_search_paths,
    }
}

/// Re-render the manifest source so that the `link { ... }` block reflects `merged`.
///
/// When the source already contains a `link` block we replace it in place (preserving every other
/// line, including comments and blank lines). When there is no block we append a freshly formatted
/// one after the last non-empty line.
pub fn render_manifest_with_link(manifest_source: &str, merged: &ProjectLinkSection) -> String {
    let block_text = render_link_block_text(merged);

    if let Some(range) = find_top_level_link_block_range(manifest_source) {
        let mut out = String::with_capacity(manifest_source.len() + block_text.len());
        out.push_str(&manifest_source[..range.0]);
        out.push_str(&block_text);
        out.push_str(&manifest_source[range.1..]);
        return out;
    }

    let mut out = manifest_source.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.ends_with("\n\n") && !out.is_empty() {
        out.push('\n');
    }
    out.push_str(&block_text);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn render_link_block_text(merged: &ProjectLinkSection) -> String {
    let mut block = String::from("link {\n");
    block.push_str(&format!(
        "  libraries = {}\n",
        format_list_literal(&merged.libraries)
    ));
    if !merged.search_paths.is_empty() {
        block.push_str(&format!(
            "  searchPaths = {}\n",
            format_string_list_literal(&merged.search_paths)
        ));
    }
    if !merged.extra_args.is_empty() {
        block.push_str(&format!(
            "  extraArgs = {}\n",
            format_string_list_literal(&merged.extra_args)
        ));
    }
    block.push('}');
    block
}

fn format_list_literal(items: &[String]) -> String {
    let mut out = String::from("[");
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        if is_safe_ident(item) {
            out.push_str(item);
        } else {
            out.push('"');
            out.push_str(item);
            out.push('"');
        }
    }
    out.push(']');
    out
}

fn format_string_list_literal(items: &[String]) -> String {
    let mut out = String::from("[");
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push('"');
        out.push_str(item);
        out.push('"');
    }
    out.push(']');
    out
}

fn is_safe_ident(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Byte range `(start, end)` of the top-level `link { ... }` block, or `None`.
fn find_top_level_link_block_range(source: &str) -> Option<(usize, usize)> {
    let document = parse_bsol_document(source).ok()?;
    document
        .blocks
        .iter()
        .find(|block| block.kind == "link")
        .map(|block| (block.span.start, block.span.end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn libc_resolution() -> LibraryResolution {
        LibraryResolution {
            provider: "c-posix".to_string(),
            host_key: "posix".to_string(),
            logical: "libc".to_string(),
            link_args: vec!["-lc".to_string()],
            search_paths: vec![],
        }
    }

    #[test]
    fn appends_link_block_when_absent() {
        let source = "project {\n  name = \"p\"\n  version = \"0.1.0\"\n}\n";
        let outcome = merge_resolution_into_manifest_source(source, None, &libc_resolution());
        assert!(!outcome.had_existing_block);
        assert!(outcome.updated_source.contains("link {"));
        assert!(outcome.updated_source.contains("libraries = [libc]"));
        assert_eq!(outcome.added_libraries, vec!["libc"]);
    }

    #[test]
    fn updates_existing_link_block_in_place() {
        let source = r#"project {
  name = "p"
  version = "0.1.0"
}

link {
  libraries = [pthread]
}
"#;
        let existing = ProjectLinkSection {
            libraries: vec!["pthread".to_string()],
            ..Default::default()
        };
        let outcome =
            merge_resolution_into_manifest_source(source, Some(&existing), &libc_resolution());
        assert!(outcome.had_existing_block);
        assert!(
            outcome
                .updated_source
                .contains("libraries = [pthread, libc]")
        );
        assert_eq!(outcome.added_libraries, vec!["libc"]);
    }

    #[test]
    fn idempotent_when_library_already_present() {
        let source = "project {\n  name = \"p\"\n  version = \"0.1.0\"\n}\n";
        let existing = ProjectLinkSection {
            libraries: vec!["libc".to_string()],
            ..Default::default()
        };
        let outcome =
            merge_resolution_into_manifest_source(source, Some(&existing), &libc_resolution());
        assert!(outcome.added_libraries.is_empty());
        assert_eq!(outcome.merged_section.libraries, vec!["libc"]);
    }

    #[test]
    fn merges_search_paths_for_path_input() {
        let source = "project {\n  name = \"p\"\n  version = \"0.1.0\"\n}\n";
        let resolution = LibraryResolution {
            provider: "c-posix".to_string(),
            host_key: "posix".to_string(),
            logical: "/usr/lib/libfoo.so".to_string(),
            link_args: vec!["/usr/lib/libfoo.so".to_string()],
            search_paths: vec![PathBuf::from("/usr/lib")],
        };
        let outcome = merge_resolution_into_manifest_source(source, None, &resolution);
        assert!(
            outcome
                .updated_source
                .contains("searchPaths = [\"/usr/lib\"]")
        );
        assert_eq!(outcome.added_search_paths, vec![PathBuf::from("/usr/lib")]);
    }

    #[test]
    fn ignores_link_substring_inside_strings() {
        // Confirms the scanner doesn't get confused by `link` appearing inside `target.entry`.
        let source = r#"project {
  name = "link-game"
  version = "0.1.0"
}
target "App" {
  kind = App
  entry = "link.bd"
}
"#;
        let outcome = merge_resolution_into_manifest_source(source, None, &libc_resolution());
        assert!(!outcome.had_existing_block);
        // The new link block is appended; no existing one was found inside strings.
        assert!(outcome.updated_source.contains("link {"));
    }
}
