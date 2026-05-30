use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use mermaid_builder::prelude::Direction as MermaidDirection;

/// Discriminator for graph domains exposed to CLI/LSP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GraphKind {
    ProjectDeps,
    Workspace,
    ModuleTree,
    ImportClosure,
    HostComposition,
}

impl GraphKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProjectDeps => "projectDeps",
            Self::Workspace => "workspace",
            Self::ModuleTree => "moduleTree",
            Self::ImportClosure => "importClosure",
            Self::HostComposition => "hostComposition",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "projectDeps" | "project" => Some(Self::ProjectDeps),
            "workspace" => Some(Self::Workspace),
            "moduleTree" | "module" => Some(Self::ModuleTree),
            "importClosure" | "imports" => Some(Self::ImportClosure),
            "hostComposition" | "host" => Some(Self::HostComposition),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GraphDirection {
    #[default]
    LeftRight,
    TopDown,
}

impl Default for GraphKind {
    fn default() -> Self {
        Self::ProjectDeps
    }
}

impl GraphDirection {
    pub fn to_mermaid(self) -> MermaidDirection {
        match self {
            Self::LeftRight => MermaidDirection::LeftToRight,
            Self::TopDown => MermaidDirection::TopToBottom,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphNodeKind {
    Root,
    Project,
    PathDependency,
    GitDependency,
    RegistryDependency,
    Module,
    Unit,
    HostRegistration,
    Scope,
    WorkspaceMember,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NodeMetadata {
    pub uri: Option<String>,
    pub project_name: Option<String>,
    pub dependency_name: Option<String>,
    pub unresolved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub kind: GraphNodeKind,
    pub style_class: Option<String>,
    pub metadata: NodeMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub label: Option<String>,
    pub style_class: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphSubgraph {
    pub id: String,
    pub label: String,
    pub node_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphWarningCode {
    Cycle,
    Unresolved,
    NoHost,
    Truncated,
    Empty,
}

impl GraphWarningCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cycle => "cycle",
            Self::Unresolved => "unresolved",
            Self::NoHost => "no_host",
            Self::Truncated => "truncated",
            Self::Empty => "empty",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphWarning {
    pub code: GraphWarningCode,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphSpec {
    pub kind: GraphKind,
    pub direction: GraphDirection,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub subgraphs: Vec<GraphSubgraph>,
    pub warnings: Vec<GraphWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphMetadata {
    pub nodes: Vec<GraphNodeSummary>,
    pub focused_project_uri: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphNodeSummary {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub uri: Option<String>,
    pub unresolved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphDocument {
    pub spec: GraphSpec,
    pub mermaid: String,
    pub revision: String,
    pub metadata: GraphMetadata,
}

impl GraphDocument {
    pub fn empty(kind: GraphKind, message: &str) -> Self {
        let spec = GraphSpec {
            kind,
            direction: GraphDirection::LeftRight,
            nodes: Vec::new(),
            edges: Vec::new(),
            subgraphs: Vec::new(),
            warnings: vec![GraphWarning {
                code: GraphWarningCode::Empty,
                message: message.to_owned(),
            }],
        };
        let revision = fingerprint_spec(&spec);
        Self {
            mermaid: format!("flowchart LR\n  empty[\"{message}\"]\n"),
            spec,
            revision,
            metadata: GraphMetadata {
                nodes: Vec::new(),
                focused_project_uri: None,
            },
        }
    }
}

impl GraphSpec {
    pub fn metadata(&self, focused_project_uri: Option<String>) -> GraphMetadata {
        GraphMetadata {
            nodes: self
                .nodes
                .iter()
                .map(|node| GraphNodeSummary {
                    id: node.id.clone(),
                    label: node.label.clone(),
                    kind: node_kind_str(node.kind).to_owned(),
                    uri: node.metadata.uri.clone(),
                    unresolved: node.metadata.unresolved,
                })
                .collect(),
            focused_project_uri,
        }
    }
}

pub fn fingerprint_spec(spec: &GraphSpec) -> String {
    let mut hasher = DefaultHasher::new();
    spec.kind.hash(&mut hasher);
    for node in &spec.nodes {
        node.id.hash(&mut hasher);
        node.label.hash(&mut hasher);
    }
    for edge in &spec.edges {
        edge.from.hash(&mut hasher);
        edge.to.hash(&mut hasher);
        edge.label.hash(&mut hasher);
    }
    for warning in &spec.warnings {
        warning.message.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

fn node_kind_str(kind: GraphNodeKind) -> &'static str {
    match kind {
        GraphNodeKind::Root => "root",
        GraphNodeKind::Project => "project",
        GraphNodeKind::PathDependency => "path",
        GraphNodeKind::GitDependency => "git",
        GraphNodeKind::RegistryDependency => "registry",
        GraphNodeKind::Module => "module",
        GraphNodeKind::Unit => "unit",
        GraphNodeKind::HostRegistration => "registration",
        GraphNodeKind::Scope => "scope",
        GraphNodeKind::WorkspaceMember => "member",
    }
}
