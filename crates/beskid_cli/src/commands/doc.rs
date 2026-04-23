use anyhow::{Context, Result};
use beskid_analysis::services;
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
    name: String,
    kind: String,
    visibility: Option<String>,
    span_start: Option<usize>,
    span_end: Option<usize>,
    doc_markdown: Option<String>,
}

#[derive(Default, Debug)]
struct TreeNode {
    children: BTreeMap<String, TreeNode>,
    entries: Vec<usize>,
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
                name: item.name.clone(),
                kind: format!("{:?}", item.kind),
                visibility: Some(format!("{:?}", item.visibility)),
                span_start: Some(item.span.start),
                span_end: Some(item.span.end),
                doc_markdown,
            });
        }
    } else {
        for symbol in services::collect_document_symbols(&snap) {
            entries.push(DocEntry {
                id: None,
                name: symbol.name,
                kind: format!("{:?}", symbol.kind),
                visibility: None,
                span_start: Some(symbol.selection_start),
                span_end: Some(symbol.selection_end),
                doc_markdown: None,
            });
        }
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name).then(a.kind.cmp(&b.kind)));

    let mut items_json = Vec::new();
    for entry in &entries {
        items_json.push(json!({
            "id": entry.id,
            "name": entry.name,
            "kind": entry.kind,
            "visibility": entry.visibility,
            "span": { "start": entry.span_start, "end": entry.span_end },
            "doc_markdown": entry.doc_markdown,
        }));
    }
    let api = json!({ "source": resolved.source_path.to_string_lossy(), "items": items_json });
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
            md.push_str(&format!("### `{}` (`{}`)\n\n", entry.name, entry.kind));
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
        let segments: Vec<&str> = entry.name.split("::").filter(|s| !s.is_empty()).collect();
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
            entry.name, entry.kind
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::{DocEntry, render_structure_tree};

    #[test]
    fn structure_tree_renders_nested_paths() {
        let entries = vec![
            DocEntry {
                id: Some(0),
                name: "util::math::sum".to_string(),
                kind: "Function".to_string(),
                visibility: None,
                span_start: None,
                span_end: None,
                doc_markdown: None,
            },
            DocEntry {
                id: Some(1),
                name: "util::math::Vec2".to_string(),
                kind: "Type".to_string(),
                visibility: None,
                span_start: None,
                span_end: None,
                doc_markdown: None,
            },
        ];

        let tree = render_structure_tree(&entries);
        assert!(tree.contains("- `util`"));
        assert!(tree.contains("- `math`"));
        assert!(tree.contains("`util::math::sum` (`Function`)"));
        assert!(tree.contains("`util::math::Vec2` (`Type`)"));
    }
}
