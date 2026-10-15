//! Compiler-owned Rust/Beskid correspondence, produced with the constructor schemas.
use super::model::{FieldMirror, ParsedType, TypeKind, VariantShape};
use crate::syntax_helpers::HelperPaths;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn fields(fields: &[FieldMirror]) -> std::io::Result<Vec<Value>> {
    fields.iter().enumerate().map(|(index, field)| {
        if field.stub_note.is_some() {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("incomplete SDK field {}", field.rust_field_source)));
        }
        Ok(json!({"ordinal":index, "beskidName":field.name, "rustName":field.rust_field_source.rsplit("::").next().unwrap(), "type":field.beskid_ty}))
    }).collect()
}

pub(super) fn syntax_schema(
    declarations: &BTreeMap<String, ParsedType>,
    helpers: &HelperPaths,
) -> std::io::Result<Value> {
    let mut types = BTreeMap::new();
    for (name, declaration) in declarations {
        if crate::syntax_traversal::is_host_only_type(name) {
            continue;
        }
        if !declaration.type_param_names.is_empty() {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "uninstantiated generic SDK schema"));
        }
        let body = match declaration.kind {
            TypeKind::Struct => json!({"kind":"record", "fields":fields(&declaration.fields)?}),
            TypeKind::Enum => {
                let variants = declaration.variants.iter().enumerate().map(|(ordinal, variant)| {
                    let (kind, payload) = match &variant.shape {
                        VariantShape::Unit => ("unit", Vec::new()),
                        VariantShape::Tuple(values) => ("tuple", fields(values)?),
                        VariantShape::Struct(values) => ("record", fields(values)?),
                    };
                    Ok(json!({"ordinal":ordinal,"beskidName":variant.name,"rustName":variant.rust_name,"kind":kind,"fields":payload}))
                }).collect::<std::io::Result<Vec<_>>>()?;
                json!({"kind":"enum", "variants":variants})
            }
        };
        types.insert(name.clone(), json!({"rustSource":declaration.source_rel_path,"body":body}));
    }
    for (name, element) in &helpers.list_helpers {
        types.insert(
            name.clone(),
            json!({"body":{"kind":"list","element":element,"storage":"managed-array-record","field":"items"}}),
        );
    }
    for (name, element) in &helpers.opt_helpers {
        types.insert(name.clone(), json!({"body":{"kind":"optional","element":element}}));
    }
    for (name, element) in &helpers.spanned_helpers {
        types.insert(
            name.clone(),
            json!({"body":{"kind":"spanned","element":element,"spanType":"NodeSpan","nodeIdType":"u32"}}),
        );
    }
    types.insert(
        "NodeSpan".into(),
        json!({"body":{"kind":"source-span","offsetType":"u64","positionType":"u64","positionBase":1}}),
    );
    types.insert("NodeRef".into(), json!({"body":{"kind":"issued-node-reference","sourceUnitType":"string","issuerType":"u64","generationType":"u64","nodeIdType":"u32"}}));
    Ok(json!({"schemaVersion":2,"module":"Beskid.Syntax.Nodes","types":types}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn v06_schema_retains_rust_variants_and_skipped_data_fields() {
        let source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../beskid_analysis/src");
        let files = crate::syntax_helpers::load_syntax_files(&source).unwrap();
        let helpers = crate::syntax_helpers::build_helper_paths(&files);
        let (declarations, _) = super::super::inventory::collect_declarations(&source, Some(&helpers)).unwrap();
        let schema = syntax_schema(&declarations, &helpers).unwrap();
        let variants = schema["types"]["Type"]["body"]["variants"].as_array().unwrap();
        assert!(variants.iter().any(|variant| variant["beskidName"] == "_This" && variant["rustName"] == "This"));
        let fields = schema["types"]["FunctionDefinition"]["body"]["fields"].as_array().unwrap();
        assert!(fields.iter().any(|field| field["beskidName"] == "whereBounds" && field["rustName"] == "where_bounds"));
        assert_eq!(schema["types"]["SpannedIdentifier"]["body"]["nodeIdType"], "u32");
        assert_eq!(schema["types"]["NodeSpan"]["body"]["offsetType"], "u64");
    }
}

#[cfg(test)]
mod closure_tests {
    #[test]
    fn v06_generated_sdk_schema_has_no_dangling_type_correspondence() {
        let source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../beskid_analysis/src");
        let files = crate::syntax_helpers::load_syntax_files(&source).unwrap();
        let helpers = crate::syntax_helpers::build_helper_paths(&files);
        let (declarations, _) = super::super::inventory::collect_declarations(&source, Some(&helpers)).unwrap();
        let schema = super::syntax_schema(&declarations, &helpers).unwrap();
        let types = schema["types"].as_object().unwrap();
        for (name, ty) in types {
            let body = &ty["body"];
            let mut references = Vec::new();
            if let Some(fields) = body["fields"].as_array() {
                references.extend(fields.iter().map(|field| field["type"].as_str().unwrap()));
            }
            if let Some(variants) = body["variants"].as_array() {
                for variant in variants {
                    references.extend(
                        variant["fields"].as_array().unwrap().iter().map(|field| field["type"].as_str().unwrap()),
                    );
                }
            }
            if let Some(element) = body["element"].as_str() {
                references.push(element);
            }
            for reference in references {
                if let Some(target) = reference.strip_prefix("Beskid.Syntax.Nodes.") {
                    assert!(types.contains_key(target), "{name} references ungenerated SDK type {target}");
                }
            }
        }
        let impl_fields = schema["types"]["ImplBlock"]["body"]["fields"].as_array().unwrap();
        for field in ["generics", "where_bounds", "method_docs"] {
            assert!(impl_fields.iter().any(|item| item["rustName"] == field));
        }
        assert!(
            schema["types"]["EnumDefinition"]["body"]["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["rustName"] == "attributes")
        );
    }
}
