use std::collections::HashMap;
use std::path::Path;

use beskid_analysis::projects::ProjectGraph;

use crate::adapters::project::from_project_graph;
use crate::compose::SpecBuilder;
use crate::model::{GraphDocument, GraphKind, GraphNodeKind, NodeMetadata};
use crate::render::{GraphError, render_document};

/// Member project graphs keyed by member display name.
pub fn from_workspace(
    workspace_name: &str,
    members: &[(String, ProjectGraph)],
) -> Result<GraphDocument, GraphError> {
    if members.is_empty() {
        return Ok(GraphDocument::empty(
            GraphKind::Workspace,
            "workspace has no members",
        ));
    }

    let mut builder = SpecBuilder::new(GraphKind::Workspace);
    let workspace_id = builder.add_node(
        workspace_name,
        GraphNodeKind::Root,
        Some("app"),
        NodeMetadata::default(),
    );

    let mut member_roots: HashMap<String, String> = HashMap::new();
    for (member_name, graph) in members {
        let member_doc = from_project_graph(graph)?;
        let root_node = member_doc
            .metadata
            .nodes
            .iter()
            .find(|n| n.kind == "root")
            .map(|n| n.label.clone())
            .unwrap_or_else(|| member_name.clone());

        let member_id = builder.add_node(
            format!("{member_name} ({root_node})"),
            GraphNodeKind::WorkspaceMember,
            Some("lib"),
            NodeMetadata {
                uri: graph.root_manifest_path.to_str().and_then(|s| {
                    Path::new(s)
                        .parent()
                        .and_then(super::super::compose::path_to_uri)
                }),
                project_name: Some(member_name.clone()),
                ..Default::default()
            },
        );
        member_roots.insert(member_name.clone(), member_id.clone());
        builder.add_edge(&workspace_id, &member_id, None, None);

        for node in &member_doc.spec.nodes {
            if node.kind == GraphNodeKind::Root {
                continue;
            }
            let id = builder.add_node(
                format!("{member_name}/{}", node.label),
                node.kind,
                node.style_class.as_deref(),
                node.metadata.clone(),
            );
            builder.add_edge(&member_id, &id, node.metadata.dependency_name.clone(), None);
        }
    }

    let _ = member_roots;
    render_document(builder.build(), None)
}
