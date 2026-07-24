//! JSON payload shape shared by LSP `beskid.getGraph` and tooling clients.

use serde_json::{Value, json};

use crate::model::{GraphDocument, GraphKind, GraphNodeSummary};

/// Build the canonical tooling payload for `beskid.getGraph`.
pub fn graph_tooling_payload(doc: &GraphDocument, kind: GraphKind, focused_project_uri: &str) -> Value {
    let warnings: Vec<Value> = doc
        .spec
        .warnings
        .iter()
        .map(|warning| {
            json!({
                "code": warning.code.as_str(),
                "message": warning.message,
            })
        })
        .collect();

    let metadata_nodes: Vec<Value> = doc.metadata.nodes.iter().map(node_summary_json).collect();

    json!({
        "kind": kind.as_str(),
        "mermaid": doc.mermaid,
        "revision": doc.revision,
        "warnings": warnings,
        "metadata": {
            "nodes": metadata_nodes,
            "focusedProjectUri": focused_project_uri,
        },
    })
}

fn node_summary_json(node: &GraphNodeSummary) -> Value {
    json!({
        "id": node.id,
        "label": node.label,
        "kind": node.kind,
        "uri": node.uri,
        "unresolved": node.unresolved,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        GraphDocument, GraphKind, GraphMetadata, GraphNodeSummary, GraphSpec, GraphWarning, GraphWarningCode,
    };

    #[test]
    fn tooling_payload_matches_contract_shape() {
        let doc = GraphDocument {
            spec: GraphSpec {
                kind: GraphKind::ProjectDeps,
                direction: Default::default(),
                nodes: Vec::new(),
                edges: Vec::new(),
                subgraphs: Vec::new(),
                warnings: vec![GraphWarning { code: GraphWarningCode::Unresolved, message: "missing-pkg".to_owned() }],
            },
            mermaid: "flowchart LR\n  n0[root]".to_owned(),
            revision: "abc123".to_owned(),
            metadata: GraphMetadata {
                nodes: vec![GraphNodeSummary {
                    id: "n0".to_owned(),
                    label: "demo".to_owned(),
                    kind: "root".to_owned(),
                    uri: Some("file:///demo/demo.bproj".to_owned()),
                    unresolved: false,
                }],
                focused_project_uri: None,
            },
        };

        let payload = graph_tooling_payload(&doc, GraphKind::ProjectDeps, "file:///demo/demo.bproj");
        assert_eq!(payload["kind"], "projectDeps");
        assert_eq!(payload["revision"], "abc123");
        assert!(payload["mermaid"].as_str().unwrap().contains("flowchart"));
        assert_eq!(payload["metadata"]["focusedProjectUri"], "file:///demo/demo.bproj");
        assert_eq!(payload["metadata"]["nodes"][0]["kind"], "root");
        assert_eq!(payload["warnings"][0]["code"], "unresolved");
    }
}
