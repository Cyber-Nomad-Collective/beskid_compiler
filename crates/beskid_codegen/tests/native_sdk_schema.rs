//! Native marshaling must consume immutable, complete generator-issued correspondence.
use beskid_codegen::native_mod::sdk_schema::CanonicalSdkSchema;

const SCHEMA: &[u8] = include_bytes!("../../../corelib/packages/compiler-sdk/src/Beskid/Syntax/syntax.schema.json");

#[test]
fn native_sdk_schema_uses_exact_generated_correspondence() {
    let schema = CanonicalSdkSchema::parse(SCHEMA).unwrap();
    assert_eq!(schema.bytes(), SCHEMA);
    assert!(schema.type_count() >= 99);
    assert_eq!(schema.field("FunctionDefinition", "where_bounds").unwrap().beskid_name(), "whereBounds");
    assert_eq!(schema.field("Parameter", "bulk").unwrap().beskid_name(), "_bulk");
    assert_eq!(schema.variant("Type", "This").unwrap().beskid_name(), "_This");
}

#[test]
fn native_sdk_schema_rejects_missing_or_forged_correspondence() {
    let mut source: serde_json::Value = serde_json::from_slice(SCHEMA).unwrap();
    source["types"].as_object_mut().unwrap().remove("SpannedIdentifier");
    assert!(CanonicalSdkSchema::parse(&serde_json::to_vec(&source).unwrap()).is_err());
    let mut source: serde_json::Value = serde_json::from_slice(SCHEMA).unwrap();
    source["types"]["NodeSpan"]["body"]["offsetType"] = "u32".into();
    assert!(CanonicalSdkSchema::parse(&serde_json::to_vec(&source).unwrap()).is_err());
    let mut source: serde_json::Value = serde_json::from_slice(SCHEMA).unwrap();
    source["types"]["FunctionDefinition"]["body"]["fields"][0]["unverifiedLayout"] = true.into();
    assert!(CanonicalSdkSchema::parse(&serde_json::to_vec(&source).unwrap()).is_err());
}
