//! BSOL board.v2 document model.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NodeKind {
    #[default]
    Panel,
    Col,
    Row,
    Split,
    Tabs,
    Stack,
}

impl NodeKind {
    pub fn parse_kind(s: &str) -> Option<Self> {
        match s {
            "col" => Some(Self::Col),
            "row" => Some(Self::Row),
            "split" => Some(Self::Split),
            "tabs" => Some(Self::Tabs),
            "stack" => Some(Self::Stack),
            "panel" => Some(Self::Panel),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Col => "col",
            Self::Row => "row",
            Self::Split => "split",
            Self::Tabs => "tabs",
            Self::Stack => "stack",
            Self::Panel => "panel",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct BoardNode {
    pub kind: NodeKind,
    pub widget: Option<String>,
    pub grow: Option<u32>,
    pub min_width: Option<u32>,
    pub min_height: Option<u32>,
    pub fixed_width: Option<u32>,
    pub fixed_height: Option<u32>,
    pub ratio: Option<u32>,
    pub children: Vec<String>,
    pub active: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BoardV2Doc {
    pub name: String,
    pub title: Option<String>,
    pub scope_hint: Option<String>,
    pub root: String,
    pub nodes: HashMap<String, BoardNode>,
}

impl BoardV2Doc {
    pub fn node(&self, id: &str) -> Option<&BoardNode> {
        self.nodes.get(id)
    }

    pub fn node_mut(&mut self, id: &str) -> Option<&mut BoardNode> {
        self.nodes.get_mut(id)
    }
}
