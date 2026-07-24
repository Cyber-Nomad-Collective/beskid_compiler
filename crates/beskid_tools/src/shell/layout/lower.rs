//! Lower `BoardV2Doc` to panes `LayoutRuntime`.

use std::sync::Arc;

use panes::runtime::LayoutRuntime;
use panes::{ContainerCtx, LayoutBuilder, fixed, grow};
use panes::{Layout, PaneError};

use super::model::{BoardNode, BoardV2Doc, NodeKind};

pub fn lower_runtime(doc: &BoardV2Doc) -> Result<LayoutRuntime, String> {
    let layout = lower_layout(doc)?;
    Ok(LayoutRuntime::from(layout))
}

pub fn lower_layout(doc: &BoardV2Doc) -> Result<Layout, String> {
    let root = doc.node(&doc.root).ok_or_else(|| "root node missing".to_string())?;
    match root.kind {
        NodeKind::Tabs => lower_tabbed(doc, root),
        NodeKind::Stack => lower_stacked(doc, root),
        NodeKind::Split => lower_split_preset(doc, root),
        _ => {
            let mut builder = LayoutBuilder::new();
            build_into_root(&mut builder, doc, &doc.root)?;
            builder.build().map_err(map_err)
        }
    }
}

fn lower_tabbed(doc: &BoardV2Doc, root: &BoardNode) -> Result<Layout, String> {
    let kinds = panel_kinds_for_children(doc, root)?;
    Layout::tabbed(kinds).build().map_err(map_err)
}

fn lower_stacked(doc: &BoardV2Doc, root: &BoardNode) -> Result<Layout, String> {
    let kinds = panel_kinds_for_children(doc, root)?;
    Layout::stacked(kinds).build().map_err(map_err)
}

fn lower_split_preset(doc: &BoardV2Doc, root: &BoardNode) -> Result<Layout, String> {
    let kids = &root.children;
    if kids.len() != 2 {
        return Err("split node requires exactly two children".into());
    }
    let first = widget_kind(doc, &kids[0])?;
    let second = widget_kind(doc, &kids[1])?;
    let mut split = Layout::split(first, second);
    if let Some(ratio) = root.ratio {
        split = split.ratio(ratio as f32 / 100.0);
    }
    split.build().map_err(map_err)
}

fn build_into_root(builder: &mut LayoutBuilder, doc: &BoardV2Doc, node_id: &str) -> Result<(), String> {
    let node = doc.node(node_id).ok_or_else(|| format!("node `{node_id}` missing"))?;
    match node.kind {
        NodeKind::Col => {
            builder.col(|ctx| build_children(ctx, doc, node)).map_err(map_err)?;
        }
        NodeKind::Row => {
            builder.row(|ctx| build_children(ctx, doc, node)).map_err(map_err)?;
        }
        NodeKind::Panel => {
            let widget = node.widget.as_deref().ok_or_else(|| format!("panel `{node_id}` missing widget"))?;
            builder
                .col(|ctx| {
                    ctx.panel_with(widget, constraints_for(node));
                })
                .map_err(map_err)?;
        }
        NodeKind::Tabs | NodeKind::Stack | NodeKind::Split => {
            return Err(format!("strategy node `{node_id}` must be root"));
        }
    }
    Ok(())
}

fn build_children(ctx: &mut ContainerCtx<'_>, doc: &BoardV2Doc, parent: &BoardNode) {
    for child_id in &parent.children {
        let _ = build_into_container(ctx, doc, child_id);
    }
}

fn build_into_container(ctx: &mut ContainerCtx<'_>, doc: &BoardV2Doc, node_id: &str) -> Result<(), String> {
    let node = doc.node(node_id).ok_or_else(|| format!("node `{node_id}` missing"))?;
    match node.kind {
        NodeKind::Col => {
            ctx.col(|c| build_children(c, doc, node));
        }
        NodeKind::Row => {
            ctx.row(|c| build_children(c, doc, node));
        }
        NodeKind::Panel => {
            let widget = node.widget.as_deref().ok_or_else(|| format!("panel `{node_id}` missing widget"))?;
            ctx.panel_with(widget, constraints_for(node));
        }
        NodeKind::Tabs | NodeKind::Stack | NodeKind::Split => {
            return Err(format!("nested strategy `{node_id}` not supported"));
        }
    }
    Ok(())
}

fn constraints_for(node: &BoardNode) -> panes::Constraints {
    if let Some(h) = node.fixed_height {
        return fixed(h as f32);
    }
    if let Some(w) = node.fixed_width {
        return fixed(w as f32);
    }
    grow(node.grow.unwrap_or(1) as f32)
}

fn panel_kinds_for_children(doc: &BoardV2Doc, root: &BoardNode) -> Result<Vec<Arc<str>>, String> {
    root.children.iter().map(|id| widget_kind(doc, id).map(Arc::from)).collect()
}

fn widget_kind(doc: &BoardV2Doc, node_id: &str) -> Result<String, String> {
    let node = doc.node(node_id).ok_or_else(|| format!("node `{node_id}` missing"))?;
    node.widget.clone().ok_or_else(|| format!("node `{node_id}` missing widget"))
}

fn map_err(e: PaneError) -> String {
    e.to_string()
}
