use std::collections::HashMap;
use std::path::PathBuf;

use crate::compose::{SpecBuilder, path_to_uri};
use crate::model::{GraphDocument, GraphKind, GraphNodeKind, NodeMetadata};
use crate::render::render_document;

/// Build an import-closure graph from unit paths and their import path strings.
pub fn from_import_closure(
    units: &[(PathBuf, Vec<String>)],
) -> Result<GraphDocument, crate::render::GraphError> {
    let mut builder = SpecBuilder::new(GraphKind::ImportClosure);
    let mut id_by_path: HashMap<String, String> = HashMap::new();

    for (path, _) in units {
        let label = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unit")
            .to_owned();
        let id = builder.add_node(
            label,
            GraphNodeKind::Unit,
            None,
            NodeMetadata {
                uri: path_to_uri(path),
                ..Default::default()
            },
        );
        id_by_path.insert(path.display().to_string(), id);
    }

    for (path, imports) in units {
        let from = match id_by_path.get(&path.display().to_string()) {
            Some(id) => id.clone(),
            None => continue,
        };
        for import in imports {
            if let Some(to) = id_by_path
                .iter()
                .find(|(p, _)| p.ends_with(import.as_str()) || p.contains(import.as_str()))
                .map(|(_, id)| id.clone())
            {
                builder.add_edge(&from, &to, Some(import.clone()), None);
            }
        }
    }

    render_document(builder.build(), None)
}
