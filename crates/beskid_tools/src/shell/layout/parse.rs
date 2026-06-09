//! Parse BSOL board documents (v2 primary, v1 import).

use std::collections::HashMap;

use bsol::{load_profile, parse_bsol_document, validate, ValidatedBlock, ValidatedDocument};

use super::model::{BoardNode, BoardV2Doc, NodeKind};
use crate::shell::board::{BoardLayout, BoardRegion, BoardTile};

pub const EMBEDDED_HI_V2: &str = include_str!("../assets/hi-default.board.v2.bsol");

pub fn parse_v2(source: &str) -> Result<BoardV2Doc, String> {
    let document = parse_bsol_document(source).map_err(|e| e.to_string())?;
    let profile = load_profile("board.v2").map_err(|e| e.to_string())?;
    let validated = validate(&document, &profile).map_err(|e| e.to_string())?;
    lower_v2(validated)
}

pub fn import_v1(layout: &BoardLayout) -> BoardV2Doc {
    let mut nodes = HashMap::new();
    let tile = |_id: &str, region: BoardRegion| -> Option<&BoardTile> {
        layout.tiles.iter().find(|t| t.region == region)
    };
    let mut panel = |id: &str, region: BoardRegion, fixed_h: Option<u32>, grow: Option<u32>| {
        if let Some(t) = tile(id, region) {
            nodes.insert(
                id.into(),
                BoardNode {
                    kind: NodeKind::Panel,
                    widget: Some(t.widget.clone()),
                    grow,
                    fixed_height: fixed_h,
                    ..BoardNode::default()
                },
            );
        }
    };
    panel("header", BoardRegion::Header, Some(4), None);
    panel("stage", BoardRegion::Stage, None, Some(1));
    panel("detail", BoardRegion::Detail, None, Some(2));
    panel("log", BoardRegion::Log, Some(8), None);
    panel("footer", BoardRegion::Footer, Some(4), None);
    nodes.insert(
        "body".into(),
        BoardNode {
            kind: NodeKind::Row,
            grow: Some(1),
            children: vec!["stage".into(), "detail".into()],
            ..BoardNode::default()
        },
    );
    nodes.insert(
        "root".into(),
        BoardNode {
            kind: NodeKind::Col,
            children: vec![
                "header".into(),
                "body".into(),
                "log".into(),
                "footer".into(),
            ],
            ..BoardNode::default()
        },
    );
    BoardV2Doc {
        name: layout.name.clone(),
        title: layout.title.clone(),
        scope_hint: layout.scope_hint.clone(),
        root: "root".into(),
        nodes,
    }
}

fn lower_v2(doc: ValidatedDocument) -> Result<BoardV2Doc, String> {
    let mut name = "default".into();
    let mut title = None;
    let mut scope_hint = None;
    let mut root = String::new();
    let mut nodes = HashMap::new();

    for block in &doc.blocks {
        match block.rule_id.as_str() {
            "board" => {
                name = block.label.clone().unwrap_or_else(|| "default".into());
                title = block.fields.get("title").cloned();
                scope_hint = block.fields.get("scope").cloned();
                root = block
                    .fields
                    .get("root")
                    .cloned()
                    .ok_or_else(|| "board missing root".to_string())?;
                let version: u32 = block
                    .fields
                    .get("version")
                    .and_then(|v| v.parse().ok())
                    .ok_or_else(|| "board.v2 requires version = 2".to_string())?;
                if version != 2 {
                    return Err(format!("unsupported board version {version}"));
                }
            }
            "node" => {
                let id = block
                    .label
                    .clone()
                    .ok_or_else(|| "node missing label".to_string())?;
                nodes.insert(id, lower_node(block)?);
            }
            other => return Err(format!("unexpected board.v2 block `{other}`")),
        }
    }

    if root.is_empty() {
        return Err("board.v2 missing root".into());
    }
    if !nodes.contains_key(&root) {
        return Err(format!("root node `{root}` not defined"));
    }

    Ok(BoardV2Doc {
        name,
        title,
        scope_hint,
        root,
        nodes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_embedded_hi_v2() {
        let doc = parse_v2(EMBEDDED_HI_V2).expect("parse v2");
        assert_eq!(doc.name, "hi-default");
        assert_eq!(doc.root, "root");
        assert!(doc.nodes.contains_key("stage"));
    }

    #[test]
    fn import_v1_produces_root() {
        let v1 = BoardLayout::parse(include_str!("../assets/hi-default.board.bsol")).expect("v1");
        let doc = import_v1(&v1);
        assert_eq!(doc.root, "root");
        assert!(doc.nodes.contains_key("header"));
    }

    #[test]
    fn emit_roundtrip_embedded_v2() {
        use super::super::emit::emit_v2;

        let doc = parse_v2(EMBEDDED_HI_V2).expect("parse");
        let text = emit_v2(&doc);
        let again = parse_v2(&text).expect("re-parse");
        assert_eq!(again.root, doc.root);
        assert_eq!(again.nodes.len(), doc.nodes.len());
    }
}

fn lower_node(block: &ValidatedBlock) -> Result<BoardNode, String> {
    let kind = block
        .fields
        .get("kind")
        .and_then(|k| NodeKind::from_str(k))
        .ok_or_else(|| format!("node `{}` has invalid kind", block.label.as_deref().unwrap_or("?")))?;
    let children = block.lists.get("children").cloned().unwrap_or_default();
    Ok(BoardNode {
        kind,
        widget: block.fields.get("widget").cloned(),
        grow: block.fields.get("grow").and_then(|v| v.parse().ok()),
        min_width: block.fields.get("min_width").and_then(|v| v.parse().ok()),
        min_height: block.fields.get("min_height").and_then(|v| v.parse().ok()),
        fixed_width: block.fields.get("fixed_width").and_then(|v| v.parse().ok()),
        fixed_height: block.fields.get("fixed_height").and_then(|v| v.parse().ok()),
        ratio: block.fields.get("ratio").and_then(|v| v.parse().ok()),
        children,
        active: block.fields.get("active").cloned(),
    })
}
