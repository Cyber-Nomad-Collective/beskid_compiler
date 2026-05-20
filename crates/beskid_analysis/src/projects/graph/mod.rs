//! Workspace [`ProjectGraph`](project_graph::ProjectGraph): discovery, dependency edges, and resolution rules.

pub mod builder;
pub mod loader;
pub mod pathing;
pub mod project_graph;
pub mod projection;
pub mod resolver;

pub use builder::{
    ProjectGraphBuildOptions, build_project_graph, build_project_graph_with_options,
    discover_workspace_resolution_rules,
};
pub use project_graph::{
    DependencyEdge, ProjectGraph, ProjectGraphNode, UnresolvedDependency, UnresolvedDependencyKind,
};
pub use projection::{collect_dependency_projects, collect_unresolved_dependencies};
pub use resolver::WorkspaceResolutionRules;
