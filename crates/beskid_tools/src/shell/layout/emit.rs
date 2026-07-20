//! Emit `BoardV2Doc` as BSOL board.v2 text.

use super::model::{BoardNode, BoardV2Doc, NodeKind};

pub fn emit_v2(doc: &BoardV2Doc) -> String {
    let mut out = String::new();
    out.push_str(&format!("board \"{}\" {{\n", escape(&doc.name)));
    out.push_str("  version = 2\n");
    if let Some(title) = &doc.title {
        out.push_str(&format!("  title = \"{}\"\n", escape(title)));
    }
    if let Some(scope) = &doc.scope_hint {
        out.push_str(&format!("  scope = {scope}\n"));
    }
    out.push_str(&format!("  root = \"{}\"\n", escape(&doc.root)));
    out.push_str("}\n");

    let mut ids: Vec<_> = doc.nodes.keys().cloned().collect();
    ids.sort();
    for id in ids {
        if let Some(node) = doc.nodes.get(&id) {
            emit_node(&mut out, &id, node);
        }
    }
    out
}

fn emit_node(out: &mut String, id: &str, node: &BoardNode) {
    out.push_str(&format!("node \"{}\" {{\n", escape(id)));
    out.push_str(&format!("  kind = {}\n", node.kind.as_str()));
    if let Some(widget) = &node.widget {
        out.push_str(&format!("  widget = \"{}\"\n", escape(widget)));
    }
    if let Some(v) = node.grow {
        out.push_str(&format!("  grow = {v}\n"));
    }
    if let Some(v) = node.min_width {
        out.push_str(&format!("  min_width = {v}\n"));
    }
    if let Some(v) = node.min_height {
        out.push_str(&format!("  min_height = {v}\n"));
    }
    if let Some(v) = node.fixed_width {
        out.push_str(&format!("  fixed_width = {v}\n"));
    }
    if let Some(v) = node.fixed_height {
        out.push_str(&format!("  fixed_height = {v}\n"));
    }
    if let Some(v) = node.ratio {
        out.push_str(&format!("  ratio = {v}\n"));
    }
    if !node.children.is_empty() {
        let joined: Vec<_> = node
            .children
            .iter()
            .map(|c| format!("\"{}\"", escape(c)))
            .collect();
        out.push_str(&format!("  children = [{}]\n", joined.join(", ")));
    }
    if let Some(active) = &node.active {
        out.push_str(&format!("  active = \"{}\"\n", escape(active)));
    }
    if matches!(node.kind, NodeKind::Panel) && node.widget.is_none() {
        // keep valid output even if incomplete during edit
    }
    out.push_str("}\n");
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
