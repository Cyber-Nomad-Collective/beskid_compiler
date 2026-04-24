use anyhow::{Context, Result};
use beskid_analysis::hir::HirVisibility;
use beskid_analysis::services;
use beskid_analysis::syntax::SpanInfo;
use clap::Args;
use serde_json::json;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct DocArgs {
    /// Beskid source file (same resolution as `analyze` when combined with `--project`)
    pub input: Option<PathBuf>,

    #[arg(long)]
    pub project: Option<PathBuf>,

    #[arg(long)]
    pub target: Option<String>,

    #[arg(long = "workspace-member")]
    pub workspace_member: Option<String>,

    #[arg(long)]
    pub frozen: bool,

    #[arg(long)]
    pub locked: bool,

    /// Output directory for `api.json` and `index.md`
    #[arg(long, default_value = "doc-out")]
    pub out: PathBuf,
}

#[derive(Clone, Debug)]
struct DocEntry {
    id: Option<usize>,
    qualified_name: String,
    kind: String,
    visibility: Option<String>,
    location: LocationJson,
    doc_markdown: Option<String>,
}

#[derive(Clone, Debug)]
struct LocationJson {
    file: String,
    start_line: usize,
    start_column: usize,
    end_line: usize,
    end_column: usize,
}

#[derive(Default, Debug)]
struct TreeNode {
    children: BTreeMap<String, TreeNode>,
    entries: Vec<usize>,
}

fn visibility_stable(vis: HirVisibility) -> &'static str {
    match vis {
        HirVisibility::Public => "public",
        HirVisibility::Private => "private",
    }
}

fn location_for_span(_source: &str, file: &str, span: &SpanInfo) -> LocationJson {
    let (sl, sc) = span.line_col_start;
    let (el, ec) = span.line_col_end;
    LocationJson {
        file: file.to_string(),
        start_line: sl,
        start_column: sc,
        end_line: el,
        end_column: ec,
    }
}

fn location_for_byte_range(source: &str, file: &str, start: usize, end: usize) -> LocationJson {
    let span = SpanInfo::from_byte_range_in_source(source, start, end);
    location_for_span(source, file, &span)
}

pub fn execute(args: DocArgs) -> Result<()> {
    let resolved = services::resolve_input(
        args.input.as_ref(),
        args.project.as_ref(),
        args.target.as_deref(),
        args.workspace_member.as_deref(),
        args.frozen,
        args.locked,
    )?;
    let program = services::parse_program_with_source_name(
        &resolved.source_path.display().to_string(),
        &resolved.source,
    )
    .with_context(|| format!("parse {}", resolved.source_path.display()))?;
    let snap = services::build_document_analysis(&program);

    let source_path_str = resolved.source_path.to_string_lossy().into_owned();

    fs::create_dir_all(&args.out).with_context(|| format!("create {}", args.out.display()))?;

    let mut entries: Vec<DocEntry> = Vec::new();
    if let Some(res) = snap.resolution.as_ref() {
        for item in &res.items {
            let doc_markdown = snap
                .item_docs
                .get(item.id.0)
                .and_then(|slot| slot.as_ref())
                .map(|d| d.markdown.clone());
            entries.push(DocEntry {
                id: Some(item.id.0),
                qualified_name: item.name.clone(),
                kind: item.kind.as_stable_doc_kind().to_string(),
                visibility: Some(visibility_stable(item.visibility).to_string()),
                location: location_for_span(&resolved.source, &source_path_str, &item.span),
                doc_markdown,
            });
        }
    } else {
        for symbol in services::collect_document_symbols(&snap) {
            entries.push(DocEntry {
                id: None,
                qualified_name: symbol.name,
                kind: services::symbol_kind_name(symbol.kind).to_string(),
                visibility: None,
                location: location_for_byte_range(
                    &resolved.source,
                    &source_path_str,
                    symbol.selection_start,
                    symbol.selection_end,
                ),
                doc_markdown: None,
            });
        }
    }
    entries.sort_by(|a, b| {
        a.qualified_name
            .cmp(&b.qualified_name)
            .then(a.kind.cmp(&b.kind))
    });

    let mut items_json = Vec::new();
    for entry in &entries {
        let loc = &entry.location;
        items_json.push(json!({
            "id": entry.id,
            "qualifiedName": entry.qualified_name,
            "name": entry.qualified_name,
            "kind": entry.kind,
            "visibility": entry.visibility,
            "location": {
                "file": loc.file,
                "startLine": loc.start_line,
                "startColumn": loc.start_column,
                "endLine": loc.end_line,
                "endColumn": loc.end_column,
            },
            "doc_markdown": entry.doc_markdown,
            "controls": [],
        }));
    }
    let api = json!({
        "schemaVersion": 1,
        "generator": concat!("beskid-cli ", env!("CARGO_PKG_VERSION")),
        "source": source_path_str,
        "items": items_json,
    });
    fs::write(
        args.out.join("api.json"),
        serde_json::to_string_pretty(&api).context("serialize api.json")?,
    )
    .with_context(|| format!("write {}", args.out.join("api.json").display()))?;

    let mut md = String::from("# API reference\n\n");
    if entries.is_empty() {
        md.push_str("*No items found.*\n");
    } else {
        md.push_str("## Structure\n\n");
        md.push_str(&render_structure_tree(&entries));
        md.push('\n');
        md.push_str("## Items\n\n");
        for entry in &entries {
            md.push_str(&format!(
                "### `{}` (`{}`)\n\n",
                entry.qualified_name, entry.kind
            ));
            let body = entry
                .doc_markdown
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("*No documentation provided.*");
            md.push_str(body);
            md.push_str("\n\n---\n\n");
        }
    }

    fs::write(args.out.join("index.md"), md)
        .with_context(|| format!("write {}", args.out.join("index.md").display()))?;

    println!(
        "Wrote {} and {}",
        args.out.join("api.json").display(),
        args.out.join("index.md").display()
    );
    Ok(())
}

fn render_structure_tree(entries: &[DocEntry]) -> String {
    let mut root = TreeNode::default();
    for (idx, entry) in entries.iter().enumerate() {
        let segments: Vec<&str> = entry
            .qualified_name
            .split("::")
            .filter(|s| !s.is_empty())
            .collect();
        if segments.is_empty() {
            root.entries.push(idx);
            continue;
        }
        let mut node = &mut root;
        for seg in &segments {
            node = node.children.entry((*seg).to_string()).or_default();
        }
        node.entries.push(idx);
    }
    let mut out = String::new();
    render_tree_node(&root, entries, 0, &mut out);
    out
}

fn render_tree_node(node: &TreeNode, entries: &[DocEntry], depth: usize, out: &mut String) {
    let indent = "  ".repeat(depth);
    for (segment, child) in &node.children {
        out.push_str(&format!("{indent}- `{segment}`\n"));
        render_tree_node(child, entries, depth + 1, out);
    }
    for entry_idx in &node.entries {
        let entry = &entries[*entry_idx];
        out.push_str(&format!(
            "{indent}- `{}` (`{}`)\n",
            entry.qualified_name, entry.kind
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::{DocEntry, LocationJson, render_structure_tree};
    use beskid_analysis::syntax::SpanInfo;

    #[test]
    fn structure_tree_renders_nested_paths() {
        let loc = LocationJson {
            file: "main.bd".into(),
            start_line: 1,
            start_column: 1,
            end_line: 1,
            end_column: 1,
        };
        let entries = vec![
            DocEntry {
                id: Some(0),
                qualified_name: "util::math::sum".to_string(),
                kind: "function".to_string(),
                visibility: None,
                location: loc.clone(),
                doc_markdown: None,
            },
            DocEntry {
                id: Some(1),
                qualified_name: "util::math::Vec2".to_string(),
                kind: "type".to_string(),
                visibility: None,
                location: loc,
                doc_markdown: None,
            },
        ];

        let tree = render_structure_tree(&entries);
        assert!(tree.contains("- `util`"));
        assert!(tree.contains("- `math`"));
        assert!(tree.contains("`util::math::sum` (`function`)"));
        assert!(tree.contains("`util::math::Vec2` (`type`)"));
    }

    #[test]
    fn location_from_byte_range_matches_line_col() {
        let src = "a\nbc\ndef";
        // "d" is third line
        let span = SpanInfo::from_byte_range_in_source(src, 5, 6);
        assert_eq!(span.line_col_start, (3, 1));
        assert_eq!(span.line_col_end, (3, 2));
    }
}
