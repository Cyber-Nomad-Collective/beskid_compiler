use mermaid_builder::prelude::*;

use crate::model::{GraphDocument, GraphSpec, fingerprint_spec};

pub type GraphError = mermaid_builder::Error;

pub fn render_document(
    spec: GraphSpec,
    focused_project_uri: Option<String>,
) -> Result<GraphDocument, GraphError> {
    let mermaid = render_flowchart(&spec)?;
    let revision = fingerprint_spec(&spec);
    let metadata = spec.metadata(focused_project_uri);
    Ok(GraphDocument {
        spec,
        mermaid,
        revision,
        metadata,
    })
}

pub fn render_flowchart(spec: &GraphSpec) -> Result<String, GraphError> {
    let config_builder =
        FlowchartConfigurationBuilder::default().direction(spec.direction.to_mermaid());

    let mut builder = FlowchartBuilder::default();
    builder = builder.configuration(config_builder)?;

    let mut node_refs = std::collections::HashMap::new();
    for node in &spec.nodes {
        let node_ref = builder.node(FlowchartNodeBuilder::default().label(&node.label)?)?;
        node_refs.insert(node.id.clone(), node_ref);
    }

    for edge in &spec.edges {
        let Some(source) = node_refs.get(&edge.from) else {
            continue;
        };
        let Some(dest) = node_refs.get(&edge.to) else {
            continue;
        };
        let mut edge_builder = FlowchartEdgeBuilder::default()
            .source(source.clone())?
            .destination(dest.clone())?
            .right_arrow_shape(ArrowShape::Normal)?;
        if let Some(label) = &edge.label {
            edge_builder = edge_builder.label(label)?;
        }
        builder.edge(edge_builder)?;
    }

    let mut output = Flowchart::from(builder).to_string();
    append_style_classes(&mut output, spec);
    Ok(output)
}

fn append_style_classes(output: &mut String, spec: &GraphSpec) {
    let mut classes = std::collections::BTreeSet::new();
    for node in &spec.nodes {
        if let Some(class) = &node.style_class {
            classes.insert(class.as_str());
        }
    }
    if classes.is_empty() {
        return;
    }
    output.push('\n');
    for class in classes {
        output.push_str(&format!("classDef {class} fill:#eef,stroke:#336;\n"));
    }
}
