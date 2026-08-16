use std::collections::HashMap;

use beskid_analysis::projects::ModuleGraph;

use crate::compose::{SpecBuilder, style_module};
use crate::model::{GraphDocument, GraphKind, GraphNodeKind, NodeMetadata};
use crate::render::render_document;

pub fn from_module_graph(module_graph: &ModuleGraph) -> Result<GraphDocument, crate::render::GraphError> {
    let mut builder = SpecBuilder::new(GraphKind::ModuleTree);
    let mut id_by_path: HashMap<Vec<String>, _> = HashMap::new();

    for module in module_graph.modules() {
        let path_label = if module.path.is_empty() { "(root)".to_owned() } else { module.path.join("::") };
        let id = builder.add_node(path_label, GraphNodeKind::Module, Some(style_module()), NodeMetadata::default());
        id_by_path.insert(module.path.clone(), id);
    }

    for module in module_graph.modules() {
        let Some(child_id) = id_by_path.get(&module.path) else { continue };
        let parent_path = &module.path[..module.path.len().saturating_sub(1)];
        if let Some(parent_id) = id_by_path.get(parent_path) {
            builder.add_edge(parent_id, child_id, None, None);
        }
    }

    render_document(builder.build(), None)
}
