//! Embedded layout templates for the hi shell editor.

use std::collections::HashMap;

use super::model::{BoardNode, BoardV2Doc, NodeKind};

pub struct LayoutTemplate {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub apply: fn(&mut BoardV2Doc),
}

pub const LAYOUT_TEMPLATES: &[LayoutTemplate] = &[
    LayoutTemplate {
        id: "holy-grail",
        title: "Holy grail",
        description: "Header, left nav, main, right aside, log, footer",
        apply: apply_holy_grail,
    },
    LayoutTemplate {
        id: "sidebar-main",
        title: "Sidebar + main",
        description: "Header, sidebar and main row, log, footer",
        apply: apply_sidebar_main,
    },
    LayoutTemplate {
        id: "single-focus",
        title: "Single focus",
        description: "Header, one full-height main panel, footer",
        apply: apply_single_focus,
    },
    LayoutTemplate {
        id: "dashboard-grid",
        title: "Dashboard grid",
        description: "Header, 2×2 widget grid, footer",
        apply: apply_dashboard_grid,
    },
];

pub fn template_by_id(id: &str) -> Option<&'static LayoutTemplate> {
    LAYOUT_TEMPLATES.iter().find(|t| t.id == id)
}

fn replace_page_root(doc: &mut BoardV2Doc, nodes: HashMap<String, BoardNode>) {
    doc.root = "root".into();
    doc.nodes = nodes;
}

fn header_panel() -> BoardNode {
    BoardNode {
        kind: NodeKind::Panel,
        widget: Some("shell.scope".into()),
        fixed_height: Some(4),
        ..BoardNode::default()
    }
}

fn log_panel() -> BoardNode {
    BoardNode { kind: NodeKind::Panel, widget: Some("shell.log".into()), fixed_height: Some(8), ..BoardNode::default() }
}

fn footer_panel() -> BoardNode {
    BoardNode {
        kind: NodeKind::Panel,
        widget: Some("shell.chrome".into()),
        fixed_height: Some(4),
        ..BoardNode::default()
    }
}

fn panel(widget: &str, grow: Option<u32>) -> BoardNode {
    BoardNode { kind: NodeKind::Panel, widget: Some(widget.into()), grow, ..BoardNode::default() }
}

fn apply_holy_grail(doc: &mut BoardV2Doc) {
    let mut nodes = HashMap::new();
    nodes.insert("header".into(), header_panel());
    nodes.insert(
        "nav_left".into(),
        BoardNode {
            kind: NodeKind::Panel,
            widget: Some("shell.shortcuts".into()),
            grow: Some(1),
            fixed_width: Some(28),
            ..BoardNode::default()
        },
    );
    nodes.insert("main".into(), panel("hi.welcome", Some(3)));
    nodes.insert(
        "aside_right".into(),
        BoardNode {
            kind: NodeKind::Panel,
            widget: Some("analysis.diagnostics".into()),
            grow: Some(1),
            fixed_width: Some(32),
            ..BoardNode::default()
        },
    );
    nodes.insert(
        "body".into(),
        BoardNode {
            kind: NodeKind::Row,
            grow: Some(1),
            children: vec!["nav_left".into(), "main".into(), "aside_right".into()],
            ..BoardNode::default()
        },
    );
    nodes.insert("log".into(), log_panel());
    nodes.insert("footer".into(), footer_panel());
    nodes.insert(
        "root".into(),
        BoardNode {
            kind: NodeKind::Col,
            children: vec!["header".into(), "body".into(), "log".into(), "footer".into()],
            ..BoardNode::default()
        },
    );
    replace_page_root(doc, nodes);
}

fn apply_sidebar_main(doc: &mut BoardV2Doc) {
    let mut nodes = HashMap::new();
    nodes.insert("header".into(), header_panel());
    nodes.insert(
        "sidebar".into(),
        BoardNode {
            kind: NodeKind::Panel,
            widget: Some("shell.shortcuts".into()),
            grow: Some(1),
            fixed_width: Some(32),
            ..BoardNode::default()
        },
    );
    nodes.insert("main".into(), panel("hi.welcome", Some(3)));
    nodes.insert(
        "body".into(),
        BoardNode {
            kind: NodeKind::Row,
            grow: Some(1),
            children: vec!["sidebar".into(), "main".into()],
            ..BoardNode::default()
        },
    );
    nodes.insert("log".into(), log_panel());
    nodes.insert("footer".into(), footer_panel());
    nodes.insert(
        "root".into(),
        BoardNode {
            kind: NodeKind::Col,
            children: vec!["header".into(), "body".into(), "log".into(), "footer".into()],
            ..BoardNode::default()
        },
    );
    replace_page_root(doc, nodes);
}

fn apply_single_focus(doc: &mut BoardV2Doc) {
    let mut nodes = HashMap::new();
    nodes.insert("header".into(), header_panel());
    nodes.insert("main".into(), panel("hi.welcome", Some(1)));
    nodes.insert("footer".into(), footer_panel());
    nodes.insert(
        "root".into(),
        BoardNode {
            kind: NodeKind::Col,
            children: vec!["header".into(), "main".into(), "footer".into()],
            ..BoardNode::default()
        },
    );
    replace_page_root(doc, nodes);
}

fn apply_dashboard_grid(doc: &mut BoardV2Doc) {
    let mut nodes = HashMap::new();
    nodes.insert("header".into(), header_panel());
    nodes.insert("tile_welcome".into(), panel("hi.welcome", Some(1)));
    nodes.insert("tile_shortcuts".into(), panel("shell.shortcuts", Some(1)));
    nodes.insert("tile_analyze".into(), panel("analysis.diagnostics", Some(1)));
    nodes.insert("tile_log".into(), panel("shell.log", Some(1)));
    nodes.insert(
        "row_top".into(),
        BoardNode {
            kind: NodeKind::Row,
            grow: Some(1),
            children: vec!["tile_welcome".into(), "tile_shortcuts".into()],
            ..BoardNode::default()
        },
    );
    nodes.insert(
        "row_bottom".into(),
        BoardNode {
            kind: NodeKind::Row,
            grow: Some(1),
            children: vec!["tile_analyze".into(), "tile_log".into()],
            ..BoardNode::default()
        },
    );
    nodes.insert(
        "grid".into(),
        BoardNode {
            kind: NodeKind::Col,
            grow: Some(1),
            children: vec!["row_top".into(), "row_bottom".into()],
            ..BoardNode::default()
        },
    );
    nodes.insert("footer".into(), footer_panel());
    nodes.insert(
        "root".into(),
        BoardNode {
            kind: NodeKind::Col,
            children: vec!["header".into(), "grid".into(), "footer".into()],
            ..BoardNode::default()
        },
    );
    replace_page_root(doc, nodes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::layout::parse::EMBEDDED_HI_V2;

    #[test]
    fn templates_replace_root_subtree() {
        let mut doc = super::super::parse::parse_v2(EMBEDDED_HI_V2).expect("parse");
        let before = doc.nodes.len();
        apply_holy_grail(&mut doc);
        assert_eq!(doc.root, "root");
        assert!(doc.nodes.contains_key("nav_left"));
        assert_ne!(before, doc.nodes.len());
    }
}
