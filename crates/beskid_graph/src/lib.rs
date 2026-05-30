//! Unified Mermaid graph presentation for Beskid compiler graph domains.

mod adapters;
mod compose;
mod model;
mod payload;
mod render;
mod validate;

pub use adapters::{
    from_composition, from_import_closure, from_module_graph, from_project_graph, from_workspace,
};
pub use compose::SpecBuilder;
pub use model::{
    GraphDocument, GraphEdge, GraphKind, GraphMetadata, GraphNode, GraphNodeKind, GraphSpec,
    GraphSubgraph, GraphWarning, GraphWarningCode, NodeMetadata,
};
pub use payload::graph_tooling_payload;
pub use render::{render_document, render_flowchart, GraphError};
pub use validate::validate_mermaid;
