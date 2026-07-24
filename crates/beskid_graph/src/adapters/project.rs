use std::collections::HashMap;

use daggy::NodeIndex;
use daggy::petgraph::visit::EdgeRef;

use beskid_analysis::projects::ProjectGraph;
use beskid_analysis::projects::graph::project_graph::ProjectGraphNode;

use crate::compose::{SpecBuilder, path_to_uri, style_class_for_project_kind, style_unresolved};
use crate::model::{GraphDocument, GraphKind, GraphNodeKind, NodeMetadata};
use crate::render::render_document;

pub fn from_project_graph(graph: &ProjectGraph) -> Result<GraphDocument, crate::render::GraphError> {
    let mut builder = SpecBuilder::new(GraphKind::ProjectDeps);
    let mut index_to_id: HashMap<NodeIndex, String> = HashMap::new();

    for index in graph.dag.graph().node_indices() {
        let Some(weight) = graph.dag.graph().node_weight(index) else {
            continue;
        };
        let (label, kind, style, metadata) = node_props(weight);
        let id = builder.add_node(label, kind, Some(style), metadata);
        index_to_id.insert(index, id);
    }

    if graph.has_std_dependency
        && !graph.dag.graph().node_weights().any(|node| {
            matches!(
                node,
                ProjectGraphNode::ResolvedPathDependency { project_name, .. }
                    if project_name == "corelib"
            )
        })
    {
        let corelib_id = builder.add_node(
            "corelib (stdlib)",
            GraphNodeKind::PathDependency,
            Some("lib"),
            NodeMetadata { project_name: Some("corelib".to_owned()), ..Default::default() },
        );
        if let Some(root_id) = index_to_id.get(&graph.root) {
            builder.add_edge(root_id, &corelib_id, Some("Std".to_owned()), None);
        }
    }

    for edge in graph.dag.graph().edge_references() {
        let Some(from) = index_to_id.get(&edge.source()).cloned() else {
            continue;
        };
        let Some(to) = index_to_id.get(&edge.target()).cloned() else {
            continue;
        };
        let label = edge.weight().dependency_name.clone();
        let style = if matches!(
            graph.dag.graph().node_weight(edge.target()),
            Some(ProjectGraphNode::UnresolvedGitDependency { .. })
                | Some(ProjectGraphNode::UnresolvedRegistryDependency { .. })
        ) {
            Some(style_unresolved())
        } else {
            None
        };
        builder.add_edge(&from, &to, Some(label), style);
    }

    let focused = path_to_uri(&graph.root_manifest_path);
    render_document(builder.build(), focused)
}

fn node_props(node: &ProjectGraphNode) -> (String, GraphNodeKind, &'static str, NodeMetadata) {
    match node {
        ProjectGraphNode::RootProject { manifest_path, project_name, project_kind, .. } => (
            project_name.clone(),
            GraphNodeKind::Root,
            style_class_for_project_kind(*project_kind),
            NodeMetadata {
                uri: path_to_uri(manifest_path),
                project_name: Some(project_name.clone()),
                ..Default::default()
            },
        ),
        ProjectGraphNode::ResolvedPathDependency {
            dependency_name, manifest_path, project_name, project_kind, ..
        } => (
            dependency_display_label(project_name, dependency_name),
            GraphNodeKind::PathDependency,
            style_class_for_project_kind(*project_kind),
            NodeMetadata {
                uri: path_to_uri(manifest_path),
                project_name: Some(project_name.clone()),
                dependency_name: Some(dependency_name.clone()),
                ..Default::default()
            },
        ),
        ProjectGraphNode::UnresolvedGitDependency { dependency_name, .. } => (
            format!("{dependency_name} (git)"),
            GraphNodeKind::GitDependency,
            style_unresolved(),
            NodeMetadata { dependency_name: Some(dependency_name.clone()), unresolved: true, ..Default::default() },
        ),
        ProjectGraphNode::UnresolvedRegistryDependency { dependency_name, .. } => (
            format!("{dependency_name} (registry)"),
            GraphNodeKind::RegistryDependency,
            style_unresolved(),
            NodeMetadata { dependency_name: Some(dependency_name.clone()), unresolved: true, ..Default::default() },
        ),
    }
}

fn dependency_display_label(project_name: &str, dependency_name: &str) -> String {
    if project_name == dependency_name {
        project_name.to_owned()
    } else {
        format!("{project_name} ({dependency_name})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn dependency_label_omits_redundant_pair() {
        assert_eq!(dependency_display_label("corelib_foundation", "corelib_foundation"), "corelib_foundation");
        assert_eq!(dependency_display_label("app", "corelib"), "app (corelib)");
    }

    #[test]
    fn project_graph_renders_mermaid() {
        let manifest =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../beskid_tests/fixtures/projects/simple_app/Project.proj");
        if !manifest.is_file() {
            return;
        }
        let graph = beskid_analysis::projects::build_project_graph(&manifest).expect("graph");
        let doc = from_project_graph(&graph).expect("render");
        assert!(doc.mermaid.contains("flowchart"));
        assert!(!doc.metadata.nodes.is_empty());
    }
}
