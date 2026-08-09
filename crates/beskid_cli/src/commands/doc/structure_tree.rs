use super::model::{DocEntry, TreeNode};

pub(super) fn render_structure_tree(entries: &[DocEntry]) -> String {
    let mut root = TreeNode::default();
    for (idx, entry) in entries.iter().enumerate() {
        let segments: Vec<&str> = entry.qualified_name.split("::").filter(|s| !s.is_empty()).collect();
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
        out.push_str(&format!("{indent}- `{}` (`{}`)\n", entry.qualified_name, entry.kind));
    }
}
