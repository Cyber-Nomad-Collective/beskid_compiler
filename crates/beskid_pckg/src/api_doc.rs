//! Typed `api.json` (schema v4 + graph navigation) embedded in `.bpk` artifacts under `.beskid/docs/`.

pub use beskid_analysis::doc::{
    API_JSON_NAVIGATION_MODEL_GRAPH_V1, API_JSON_SCHEMA_VERSION,
    API_JSON_SCHEMA_VERSION_BEFORE_GRAPH, ApiDocRoot,
};

fn validate_type_ref(target: Option<usize>, root: &ApiDocRoot, item_label: &str) -> Result<(), String> {
    let Some(ref_id) = target else {
        return Ok(());
    };
    if root.items.iter().any(|p| p.id == Some(ref_id)) {
        Ok(())
    } else {
        Err(format!(
            "{item_label} references missing refItemId {ref_id}"
        ))
    }
}

/// Validates that a packed `api.json` satisfies the graph navigation contract for schema v3+.
pub fn validate_packed_api_doc(root: &ApiDocRoot) -> Result<(), String> {
    if root.schema_version >= API_JSON_SCHEMA_VERSION {
        if root.navigation_model.as_deref() != Some(API_JSON_NAVIGATION_MODEL_GRAPH_V1) {
            return Err(format!(
                "schemaVersion {} requires navigationModel \"{}\"",
                root.schema_version, API_JSON_NAVIGATION_MODEL_GRAPH_V1
            ));
        }

        for item in &root.items {
            let Some(id) = item.id else {
                return Err(format!(
                    "graph api.json requires id on every item (missing on \"{}\")",
                    item.qualified_name
                ));
            };

            if let Some(pid) = item.parent_id {
                let parent_exists = root.items.iter().any(|p| p.id == Some(pid));
                if !parent_exists {
                    return Err(format!(
                        "item id {id} (\"{}\") references missing parentId {pid}",
                        item.qualified_name
                    ));
                }
            }

            validate_type_ref(
                item.field_type.as_ref().and_then(|t| t.ref_item_id),
                root,
                &format!("item id {id} (\"{}\") fieldType", item.qualified_name),
            )?;
            validate_type_ref(
                item.return_type.as_ref().and_then(|t| t.ref_item_id),
                root,
                &format!("item id {id} (\"{}\") returnType", item.qualified_name),
            )?;
            for (index, param) in item.parameters.iter().enumerate() {
                validate_type_ref(
                    param.ty.ref_item_id,
                    root,
                    &format!(
                        "item id {id} (\"{}\") parameters[{index}].type",
                        item.qualified_name
                    ),
                )?;
            }
        }
    } else if root.schema_version > API_JSON_SCHEMA_VERSION_BEFORE_GRAPH {
        return Err(format!(
            "unsupported api.json schemaVersion {} (max supported: {})",
            root.schema_version, API_JSON_SCHEMA_VERSION
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use beskid_analysis::doc::{ApiDocItem, ApiLocation};

    fn sample_location() -> ApiLocation {
        ApiLocation {
            file: "t.bd".into(),
            start_line: 1,
            start_column: 1,
            end_line: 1,
            end_column: 1,
        }
    }

    fn minimal_item(
        id: usize,
        name: &str,
        kind: &str,
        parent_id: Option<usize>,
        member_ids: Vec<usize>,
    ) -> ApiDocItem {
        ApiDocItem {
            id: Some(id),
            qualified_name: name.into(),
            name: name.into(),
            display_name: None,
            kind: kind.into(),
            visibility: Some("public".into()),
            location: sample_location(),
            parent_id,
            member_ids,
            module_path: vec![],
            signature: None,
            field_type: None,
            return_type: None,
            parameters: vec![],
            generic_parameters: vec![],
            doc_markdown: None,
            doc: None,
            controls: vec![],
        }
    }

    #[test]
    fn rejects_v3_without_graph_navigation_model() {
        let root = ApiDocRoot {
            schema_version: API_JSON_SCHEMA_VERSION,
            navigation_model: None,
            generator: "test".into(),
            source: "t.bd".into(),
            items: vec![minimal_item(1, "Root", "module", None, vec![])],
        };
        let err = validate_packed_api_doc(&root).expect_err("expected error");
        assert!(err.contains("navigationModel"));
    }

    #[test]
    fn accepts_v3_graph_with_parent_link() {
        let root = ApiDocRoot {
            schema_version: API_JSON_SCHEMA_VERSION,
            navigation_model: Some(API_JSON_NAVIGATION_MODEL_GRAPH_V1.into()),
            generator: "test".into(),
            source: "t.bd".into(),
            items: vec![
                minimal_item(1, "T", "type", None, vec![2]),
                minimal_item(2, "T::x", "field", Some(1), vec![]),
            ],
        };
        validate_packed_api_doc(&root).expect("valid graph");
    }
}
