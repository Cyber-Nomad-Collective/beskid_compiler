use crate::model::{
    GraphDirection, GraphEdge, GraphKind, GraphNode, GraphNodeKind, GraphSpec, GraphSubgraph,
    GraphWarning, GraphWarningCode, NodeMetadata,
};

/// Accumulates a [`GraphSpec`] with shared node-id sanitization and style helpers.
#[derive(Debug, Default)]
pub struct SpecBuilder {
    kind: GraphKind,
    direction: GraphDirection,
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    subgraphs: Vec<GraphSubgraph>,
    warnings: Vec<GraphWarning>,
    next_id: u32,
}

impl SpecBuilder {
    pub fn new(kind: GraphKind) -> Self {
        Self {
            kind,
            ..Default::default()
        }
    }

    pub fn direction(mut self, direction: GraphDirection) -> Self {
        self.direction = direction;
        self
    }

    pub fn warn(mut self, code: GraphWarningCode, message: impl Into<String>) -> Self {
        self.warnings.push(GraphWarning {
            code,
            message: message.into(),
        });
        self
    }

    pub fn add_node(
        &mut self,
        label: impl Into<String>,
        kind: GraphNodeKind,
        style_class: Option<&str>,
        metadata: NodeMetadata,
    ) -> String {
        let label = label.into();
        let id = format!("n{}", self.next_id);
        self.next_id += 1;
        self.nodes.push(GraphNode {
            id: id.clone(),
            label,
            kind,
            style_class: style_class.map(str::to_owned),
            metadata,
        });
        id
    }

    pub fn add_edge(
        &mut self,
        from: &str,
        to: &str,
        label: Option<String>,
        style_class: Option<&str>,
    ) {
        self.edges.push(GraphEdge {
            from: from.to_owned(),
            to: to.to_owned(),
            label,
            style_class: style_class.map(str::to_owned),
        });
    }

    pub fn add_subgraph(&mut self, label: impl Into<String>, node_ids: Vec<String>) -> String {
        let label = label.into();
        let id = sanitize_id(&label);
        self.subgraphs.push(GraphSubgraph {
            id: id.clone(),
            label,
            node_ids,
        });
        id
    }

    pub fn build(self) -> GraphSpec {
        GraphSpec {
            kind: self.kind,
            direction: self.direction,
            nodes: self.nodes,
            edges: self.edges,
            subgraphs: self.subgraphs,
            warnings: self.warnings,
        }
    }
}

pub fn sanitize_id(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    for ch in label.chars() {
        match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '_' => out.push(ch),
            _ => out.push('_'),
        }
    }
    if out.is_empty() {
        "node".to_owned()
    } else if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("n_{out}")
    } else {
        out
    }
}

pub fn style_class_for_project_kind(kind: beskid_analysis::projects::ProjectKind) -> &'static str {
    use beskid_analysis::projects::ProjectKind;
    match kind {
        ProjectKind::Host => "host",
        ProjectKind::Mod => "mod",
        ProjectKind::Template => "template",
    }
}

pub fn style_unresolved() -> &'static str {
    "unresolved"
}

pub fn style_host_registration() -> &'static str {
    "hostReg"
}

pub fn style_module() -> &'static str {
    "module"
}

pub fn path_to_uri(path: &std::path::Path) -> Option<String> {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    url_from_path(&canonical)
}

fn url_from_path(path: &std::path::Path) -> Option<String> {
    let path_str = path.display().to_string();
    if cfg!(windows) {
        Some(format!("file:///{}", path_str.replace('\\', "/")))
    } else {
        Some(format!("file://{path_str}"))
    }
}
