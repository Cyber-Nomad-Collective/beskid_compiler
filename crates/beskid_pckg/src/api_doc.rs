//! Typed `api.json` (schema v4 + graph navigation) embedded in `.bpk` artifacts under `.beskid/docs/`.

pub use beskid_analysis::doc::{
    API_JSON_NAVIGATION_MODEL_GRAPH_V1, API_JSON_SCHEMA_VERSION,
    API_JSON_SCHEMA_VERSION_BEFORE_GRAPH, ApiDocRoot, path_looks_absolute,
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

fn validate_package_relative_paths(root: &ApiDocRoot) -> Result<(), String> {
    if path_looks_absolute(&root.source) {
        return Err(format!(
            "api.json source must be package-relative, not absolute: \"{}\"",
            root.source
        ));
    }
    for item in &root.items {
        if path_looks_absolute(&item.location.file) {
            return Err(format!(
                "api.json item \"{}\" location.file must be package-relative, not absolute: \"{}\"",
                item.qualified_name, item.location.file
            ));
        }
    }
    Ok(())
}

/// Validates that a packed `api.json` satisfies the graph navigation contract for schema v3+.
pub fn validate_packed_api_doc(root: &ApiDocRoot) -> Result<(), String> {
    validate_package_relative_paths(root)?;

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

            validate_library_tree_row(item)?;
        }

        validate_symbol_keys(root)?;
        validate_library_tree_aggregate(root)?;
    } else if root.schema_version > API_JSON_SCHEMA_VERSION_BEFORE_GRAPH {
        return Err(format!(
            "unsupported api.json schemaVersion {} (max supported: {})",
            root.schema_version, API_JSON_SCHEMA_VERSION
        ));
    }

    Ok(())
}

const KIND_MODULE: &str = "module";
const MODULE_LEVEL_KINDS: &[&str] = &["type", "enum", "contract", "function", "test"];

fn validate_library_tree_row(item: &beskid_analysis::doc::ApiDocItem) -> Result<(), String> {
    let Some(id) = item.id else {
        return Ok(());
    };
    if item.kind == KIND_MODULE {
        let depth = item.module_path.len();
        if depth > 1 && item.parent_id.is_none() {
            return Err(format!(
                "module id {id} (\"{}\") with modulePath depth {depth} requires parentId",
                item.qualified_name
            ));
        }
    }
    if MODULE_LEVEL_KINDS.contains(&item.kind.as_str()) && item.parent_id.is_none() {
        let has_module_path = !item.module_path.is_empty()
            || item.qualified_name.contains("::");
        if has_module_path {
            return Err(format!(
                "item id {id} (\"{}\") kind \"{}\" at module scope requires parentId to its module",
                item.qualified_name, item.kind
            ));
        }
    }
    Ok(())
}

fn validate_symbol_keys(root: &ApiDocRoot) -> Result<(), String> {
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for item in &root.items {
        let Some(ref key) = item.symbol_key else {
            continue;
        };
        let key_str = key.as_str();
        if key_str.trim().is_empty() {
            let label = item
                .id
                .map(|id| format!("id {id}"))
                .unwrap_or_else(|| item.qualified_name.clone());
            return Err(format!("api.json item \"{label}\" has empty symbolKey"));
        }
        if !key_str.contains("::") {
            let label = item
                .id
                .map(|id| format!("id {id} (\"{}\")", item.qualified_name))
                .unwrap_or_else(|| item.qualified_name.clone());
            return Err(format!(
                "api.json item {label} symbolKey must be package-prefixed (contain \"::\"), got \"{key_str}\""
            ));
        }
        if let Some(&previous_id) = seen.get(key_str) {
            let id = item.id.unwrap_or(0);
            return Err(format!(
                "duplicate symbolKey \"{key_str}\" on item id {id} and id {previous_id}"
            ));
        }
        if let Some(id) = item.id {
            seen.insert(key_str.to_string(), id);
        }
        if let Some((package, rest)) = key_str.split_once("::")
            && !package.is_empty()
            && !rest.is_empty()
            && !item.qualified_name.is_empty()
        {
            let qn_leaf = item
                .qualified_name
                .rsplit("::")
                .next()
                .unwrap_or(item.qualified_name.as_str());
            let key_leaf = rest.rsplit("::").next().unwrap_or(rest);
            if qn_leaf != key_leaf
                && !item.qualified_name.ends_with(&format!("::{key_leaf}"))
                && !rest.ends_with(&format!("::{qn_leaf}"))
            {
                return Err(format!(
                    "item id {} (\"{}\") symbolKey leaf \"{key_leaf}\" contradicts qualifiedName leaf \"{qn_leaf}\"",
                    item.id.unwrap_or(0),
                    item.qualified_name
                ));
            }
        }
    }
    Ok(())
}

fn validate_library_tree_aggregate(root: &ApiDocRoot) -> Result<(), String> {
    const MAX_GRAPH_ROOTS: usize = 128;
    let roots = root
        .items
        .iter()
        .filter(|i| i.parent_id.is_none())
        .count();
    if roots > MAX_GRAPH_ROOTS {
        return Err(format!(
            "api.json has {roots} graph roots (max {MAX_GRAPH_ROOTS}); re-run beskid doc with a current CLI to link the module library tree"
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
        minimal_item_with_symbol_key(id, name, kind, parent_id, member_ids, None)
    }

    fn minimal_item_with_symbol_key(
        id: usize,
        name: &str,
        kind: &str,
        parent_id: Option<usize>,
        member_ids: Vec<usize>,
        symbol_key: Option<beskid_analysis::doc::ApiSymbolKey>,
    ) -> ApiDocItem {
        ApiDocItem {
            id: Some(id),
            qualified_name: name.into(),
            symbol_key,
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
            declaring_package: None,
            controls: vec![],
            tier: None,
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
    fn rejects_absolute_source_path() {
        let root = ApiDocRoot {
            schema_version: API_JSON_SCHEMA_VERSION,
            navigation_model: Some(API_JSON_NAVIGATION_MODEL_GRAPH_V1.into()),
            generator: "test".into(),
            source: "/tmp/pkg/src/Main.bd".into(),
            items: vec![minimal_item(1, "Root", "module", None, vec![])],
        };
        let err = validate_packed_api_doc(&root).expect_err("expected error");
        assert!(err.contains("package-relative"));
    }

    #[test]
    fn rejects_windows_drive_source_path() {
        let root = ApiDocRoot {
            schema_version: API_JSON_SCHEMA_VERSION,
            navigation_model: Some(API_JSON_NAVIGATION_MODEL_GRAPH_V1.into()),
            generator: "test".into(),
            source: "C:\\pkg\\src\\Main.bd".into(),
            items: vec![minimal_item(1, "Root", "module", None, vec![])],
        };
        let err = validate_packed_api_doc(&root).expect_err("expected error");
        assert!(err.contains("package-relative"));
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

    #[test]
    fn accepts_v4_graph_with_distinct_symbol_keys() {
        let root = ApiDocRoot {
            schema_version: API_JSON_SCHEMA_VERSION,
            navigation_model: Some(API_JSON_NAVIGATION_MODEL_GRAPH_V1.into()),
            generator: "test".into(),
            source: "t.bd".into(),
            items: vec![
                minimal_item_with_symbol_key(
                    1,
                    "Root",
                    "module",
                    None,
                    vec![],
                    Some(beskid_analysis::doc::ApiSymbolKey::new("demo::Root")),
                ),
                minimal_item_with_symbol_key(
                    2,
                    "Root::fn",
                    "function",
                    Some(1),
                    vec![],
                    Some(beskid_analysis::doc::ApiSymbolKey::new("demo::Root::fn")),
                ),
            ],
        };
        validate_packed_api_doc(&root).expect("valid symbol keys");
    }

    #[test]
    fn rejects_duplicate_symbol_key() {
        let root = ApiDocRoot {
            schema_version: API_JSON_SCHEMA_VERSION,
            navigation_model: Some(API_JSON_NAVIGATION_MODEL_GRAPH_V1.into()),
            generator: "test".into(),
            source: "t.bd".into(),
            items: vec![
                minimal_item_with_symbol_key(
                    1,
                    "Mod",
                    "module",
                    None,
                    vec![2, 3],
                    Some(beskid_analysis::doc::ApiSymbolKey::new("demo::Mod")),
                ),
                minimal_item_with_symbol_key(
                    2,
                    "Shared",
                    "type",
                    Some(1),
                    vec![],
                    Some(beskid_analysis::doc::ApiSymbolKey::new("demo::Shared")),
                ),
                minimal_item_with_symbol_key(
                    3,
                    "Shared",
                    "enum",
                    Some(1),
                    vec![],
                    Some(beskid_analysis::doc::ApiSymbolKey::new("demo::Shared")),
                ),
            ],
        };
        let err = validate_packed_api_doc(&root).expect_err("expected error");
        assert!(err.contains("duplicate symbolKey"), "got: {err}");
    }

    #[test]
    fn rejects_symbol_key_without_package_prefix() {
        let root = ApiDocRoot {
            schema_version: API_JSON_SCHEMA_VERSION,
            navigation_model: Some(API_JSON_NAVIGATION_MODEL_GRAPH_V1.into()),
            generator: "test".into(),
            source: "t.bd".into(),
            items: vec![minimal_item_with_symbol_key(
                1,
                "Root",
                "module",
                None,
                vec![],
                Some(beskid_analysis::doc::ApiSymbolKey::new("RootOnly")),
            )],
        };
        let err = validate_packed_api_doc(&root).expect_err("expected error");
        assert!(err.contains("package-prefixed"));
    }

    #[test]
    fn accepts_items_without_symbol_key() {
        let root = ApiDocRoot {
            schema_version: API_JSON_SCHEMA_VERSION,
            navigation_model: Some(API_JSON_NAVIGATION_MODEL_GRAPH_V1.into()),
            generator: "test".into(),
            source: "t.bd".into(),
            items: vec![minimal_item(1, "Root", "module", None, vec![])],
        };
        validate_packed_api_doc(&root).expect("symbolKey remains optional");
    }
}
