use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use crate::model::{GraphDocument, GraphSpec, fingerprint_spec};

#[derive(Debug, Error)]
pub enum GraphError {
    #[error("graph edge references unknown node `{0}`")]
    UnknownNode(String),
}

pub fn render_document(spec: GraphSpec, focused_project_uri: Option<String>) -> Result<GraphDocument, GraphError> {
    let mermaid = render_flowchart(&spec)?;
    let revision = fingerprint_spec(&spec);
    let metadata = spec.metadata(focused_project_uri);
    Ok(GraphDocument { spec, mermaid, revision, metadata })
}

pub fn render_flowchart(spec: &GraphSpec) -> Result<String, GraphError> {
    let node_ids: BTreeSet<&str> = spec.nodes.iter().map(|node| node.id.as_str()).collect();

    let mut output = format!("flowchart {}\n", spec.direction.to_mermaid_direction());

    for node in &spec.nodes {
        output.push_str(&format!("  {}[{}]\n", node.id, sanitize_tui_label(&node.label)));
    }

    for edge in &spec.edges {
        if !node_ids.contains(edge.from.as_str()) {
            return Err(GraphError::UnknownNode(edge.from.clone()));
        }
        if !node_ids.contains(edge.to.as_str()) {
            return Err(GraphError::UnknownNode(edge.to.clone()));
        }

        match &edge.label {
            Some(label) => {
                output.push_str(&format!("  {} -->|{}| {}\n", edge.from, escape_edge_label(label), edge.to));
            }
            None => {
                output.push_str(&format!("  {} --> {}\n", edge.from, edge.to));
            }
        }
    }

    append_style_classes(&mut output, spec);
    Ok(output)
}

fn sanitize_tui_label(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    for ch in label.chars() {
        match ch {
            '(' => out.push_str(" · "),
            ')' | '{' | '}' => {}
            '[' => out.push('⟨'),
            ']' => out.push('⟩'),
            '|' => out.push('¦'),
            other => out.push(other),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn escape_edge_label(label: &str) -> String {
    label.replace('|', "\\|")
}

fn append_style_classes(output: &mut String, spec: &GraphSpec) {
    let mut classes = BTreeSet::new();
    let mut nodes_by_class: BTreeMap<&str, Vec<&str>> = BTreeMap::new();

    for node in &spec.nodes {
        if let Some(class) = node.style_class.as_deref() {
            classes.insert(class);
            nodes_by_class.entry(class).or_default().push(&node.id);
        }
    }

    if classes.is_empty() {
        return;
    }

    output.push('\n');
    for class in classes {
        output.push_str(&format!("classDef {class} fill:#eef,stroke:#336;\n"));
    }
    for (class, node_ids) in nodes_by_class {
        output.push_str(&format!("class {} {}\n", node_ids.join(","), class));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GraphDirection, GraphEdge, GraphKind, GraphNode, GraphNodeKind, GraphSpec, NodeMetadata};
    use graphs_tui::{RenderOptions, render_mermaid_to_tui};

    #[test]
    fn flowchart_uses_classic_node_syntax() {
        let spec = GraphSpec {
            kind: GraphKind::ProjectDeps,
            direction: GraphDirection::LeftRight,
            nodes: vec![
                GraphNode {
                    id: "n0".to_owned(),
                    label: "corelib".to_owned(),
                    kind: GraphNodeKind::Root,
                    style_class: Some("host".to_owned()),
                    metadata: NodeMetadata::default(),
                },
                GraphNode {
                    id: "n1".to_owned(),
                    label: "app".to_owned(),
                    kind: GraphNodeKind::Project,
                    style_class: None,
                    metadata: NodeMetadata::default(),
                },
            ],
            edges: vec![GraphEdge {
                from: "n0".to_owned(),
                to: "n1".to_owned(),
                label: Some("depends".to_owned()),
                style_class: None,
            }],
            subgraphs: Vec::new(),
            warnings: Vec::new(),
        };

        let mermaid = render_flowchart(&spec).expect("render");
        assert!(mermaid.contains("flowchart LR"));
        assert!(mermaid.contains("n0[corelib]"));
        assert!(mermaid.contains("n0 -->|depends| n1"));
        assert!(mermaid.contains("classDef host"));
        assert!(mermaid.contains("class n0 host"));

        render_mermaid_to_tui(&mermaid, RenderOptions::default()).expect("tui render");
    }

    #[test]
    fn flowchart_labels_with_parentheses_render_for_tui() {
        let spec = GraphSpec {
            kind: GraphKind::ProjectDeps,
            direction: GraphDirection::LeftRight,
            nodes: vec![GraphNode {
                id: "n1".to_owned(),
                label: "corelib_foundation (corelib_foundation)".to_owned(),
                kind: GraphNodeKind::PathDependency,
                style_class: None,
                metadata: NodeMetadata::default(),
            }],
            edges: Vec::new(),
            subgraphs: Vec::new(),
            warnings: Vec::new(),
        };

        let mermaid = render_flowchart(&spec).expect("render");
        assert!(mermaid.contains("n1[corelib_foundation · corelib_foundation]"));
        render_mermaid_to_tui(&mermaid, RenderOptions::default()).expect("tui render");
    }
}
