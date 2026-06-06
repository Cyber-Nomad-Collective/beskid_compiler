use std::collections::HashMap;

use beskid_analysis::resolve::ModuleGraph;

use crate::compose::{SpecBuilder, style_module};
use crate::model::{GraphDocument, GraphKind, GraphNodeKind, NodeMetadata};
use crate::render::render_document;

pub fn from_module_graph(
    module_graph: &ModuleGraph,
) -> Result<GraphDocument, crate::render::GraphError> {
    let mut builder = SpecBuilder::new(GraphKind::ModuleTree);
    let mut id_by_module = HashMap::new();

    for module in module_graph.modules() {
        let path_label = if module.path.is_empty() {
            "(root)".to_owned()
        } else {
            module.path.join("::")
        };
        let id = builder.add_node(
            path_label,
            GraphNodeKind::Module,
            Some(style_module()),
            NodeMetadata::default(),
        );
        id_by_module.insert(module.id, id);
    }

    for module in module_graph.modules() {
        let Some(from_id) = id_by_module.get(&module.id) else {
            continue;
        };
        if let Some(parent) = module.parent
            && let Some(parent_id) = id_by_module.get(&parent)
        {
            builder.add_edge(parent_id, from_id, None, None);
        }
    }

    render_document(builder.build(), None)
}
