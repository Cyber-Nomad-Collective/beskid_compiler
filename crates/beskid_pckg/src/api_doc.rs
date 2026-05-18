//! Typed `api.json` (schema v3 + graph navigation) embedded in `.bpk` artifacts under `.beskid/docs/`.

pub use beskid_analysis::doc::{
    API_JSON_NAVIGATION_MODEL_GRAPH_V1, API_JSON_SCHEMA_VERSION, API_JSON_SCHEMA_VERSION_BEFORE_GRAPH,
    ApiDocRoot,
};

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

    #[test]
    fn rejects_v3_without_graph_navigation_model() {
        let root = ApiDocRoot {
            schema_version: API_JSON_SCHEMA_VERSION,
            navigation_model: None,
            generator: "test".into(),
            source: "t.bd".into(),
            items: vec![ApiDocItem {
                id: Some(1),
                qualified_name: "Root".into(),
                name: "Root".into(),
                kind: "module".into(),
                visibility: Some("public".into()),
                location: sample_location(),
                parent_id: None,
                member_ids: vec![],
                doc_markdown: None,
                doc: None,
                controls: vec![],
            }],
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
                ApiDocItem {
                    id: Some(1),
                    qualified_name: "T".into(),
                    name: "T".into(),
                    kind: "type".into(),
                    visibility: Some("public".into()),
                    location: sample_location(),
                    parent_id: None,
                    member_ids: vec![2],
                    doc_markdown: None,
                    doc: None,
                    controls: vec![],
                },
                ApiDocItem {
                    id: Some(2),
                    qualified_name: "T::x".into(),
                    name: "x".into(),
                    kind: "field".into(),
                    visibility: Some("public".into()),
                    location: sample_location(),
                    parent_id: Some(1),
                    member_ids: vec![],
                    doc_markdown: None,
                    doc: None,
                    controls: vec![],
                },
            ],
        };
        validate_packed_api_doc(&root).expect("valid graph");
    }
}
