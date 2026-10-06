//! Shared Salsa authority for logical Glue signatures and closed metadata.
#[path = "codegen_input/support.rs"]
#[allow(unused_imports, dead_code)]
mod support;
use beskid_analysis::syntax::PrimitiveType;
use beskid_queries::{AstNodeKey, GlueLogicalType, IndexedNodeKind, child_nodes, glue_binding, item_name, node_kind};
fn function(db: &dyn beskid_queries::Db, key: AstNodeKey, name: &str) -> Option<AstNodeKey> {
    if node_kind(db, key).ok().flatten() == Some(IndexedNodeKind::FunctionDefinition)
        && item_name(db, key).ok().flatten().is_some_and(|n| n.as_ref() == name)
    {
        return Some(key);
    }
    child_nodes(db, key).ok().flatten()?.iter().copied().find_map(|c| function(db, c, name))
}
fn fact(source: &str, name: &str) -> Result<beskid_queries::GlueBindingFact, String> {
    let (db, _typed, root, _target) = support::input_fixture_with_source(source);
    let key = function(&db, root, name).expect("real registered function");
    glue_binding(&db, key).map_err(|e| e.to_string())?.ok_or_else(|| "absent binding".into())
}
#[test]
fn canonical_glue_facts_preserve_logical_managed_and_scalar_identity() {
    let source = include_str!("../../beskid_tests_interop/fixtures/glue/manual/src/ManualExport.bd");
    let (db, _typed, root, _target) = support::input_fixture_with_source(source);
    for (name, expected) in [
        ("ExportBOOL", GlueLogicalType::Primitive(PrimitiveType::Bool)),
        ("ExportCHAR", GlueLogicalType::Primitive(PrimitiveType::Char)),
        ("ExportUTF8", GlueLogicalType::Primitive(PrimitiveType::String)),
        ("ExportBYTES", GlueLogicalType::Array(Box::new(GlueLogicalType::Primitive(PrimitiveType::U8)))),
        ("ExportHANDLE", GlueLogicalType::Primitive(PrimitiveType::U64)),
    ] {
        let key = function(&db, root, name).unwrap();
        let value = glue_binding(&db, key).unwrap().unwrap();
        assert_eq!(value.declaration, key);
        assert_eq!(value.parameters[0].ty, expected);
        assert_eq!(value.result, expected);
    }
}
#[test]
fn canonical_glue_metadata_decodes_escapes_and_rejects_decoded_nul() {
    let valid = r#"[Export(Abi:"C", Symbol:"escaped\"symbol")] pub i32 ExportI32(i32 value) { return value; }"#;
    assert_eq!(fact(valid, "ExportI32").unwrap().symbol, "escaped\"symbol");
    let invalid = "[Export(Abi:\"C\", Symbol:\"bad\0symbol\")] pub i32 ExportI32(i32 value) { return value; }";
    assert!(fact(invalid, "ExportI32").unwrap_err().contains("metadata"));
}
#[test]
fn canonical_glue_metadata_is_closed_and_unique() {
    for source in [
        r#"[Export(Abi:"C", Symbol:"one", Symbol:"two")] pub i32 ExportI32(i32 value) {return value;}"#,
        r#"[Export(Abi:"C", Symbol:"one", Guess:true)] pub i32 ExportI32(i32 value) {return value;}"#,
        r#"[Export(Abi:"C", Symbol:"one")] [Export(Abi:"C", Symbol:"two")] pub i32 ExportI32(i32 value) {return value;}"#,
    ] {
        assert!(fact(source, "ExportI32").is_err());
    }
}
#[test]
fn canonical_glue_type_closure_bounds_wide_signatures() {
    let parameters =
        (0..200).map(|i| format!("Token<Token<Token<Token<Token<u8>>>>> arg{i}")).collect::<Vec<_>>().join(",");
    let source = format!(
        "[GlueHandle(Library:\"manual\")] pub type Token<T> {{ u64 token, }} [Export(Abi:\"C\",Symbol:\"wide\")] pub unit ExportWide({parameters}) {{return;}}"
    );
    assert!(fact(&source, "ExportWide").unwrap_err().contains("closure"));
}
#[test]
fn canonical_glue_generic_reference_keeps_its_declaration_and_position() {
    let source = r#"[Export(Abi:"C",Symbol:"generic")] pub T ExportGeneric<T>(T value) {return value;}"#;
    let binding = fact(source, "ExportGeneric").unwrap();
    assert_eq!(
        binding.parameters[0].ty,
        GlueLogicalType::Generic { declaration: binding.declaration, position: 0, name: "T".into() }
    );
}
#[test]
fn canonical_glue_handle_brand_retains_qualified_owner_and_arguments() {
    let source = r#"[GlueHandle(Library:"manual",Nullable:false)] pub type Token<T> { u64 token, }
    [Export(Abi:"C",Symbol:"handle")] pub Token<i32> ExportHandle(Token<i32> value) {return value;}"#;
    let binding = fact(source, "ExportHandle").unwrap();
    let GlueLogicalType::Handle(brand) = &binding.parameters[0].ty else {
        panic!("explicit nominal handle brand required")
    };
    assert_eq!(brand.library, "manual");
    assert!(!brand.nullable);
    assert!(!brand.qualified_identity.is_empty());
    assert_eq!(brand.arguments.as_ref(), &[GlueLogicalType::Primitive(PrimitiveType::I32)]);
    assert_ne!(brand.declaration, binding.declaration);
}

#[test]
fn opaque_shape_projection_preserves_applied_arguments_without_scalar_brand_grants() {
    let source=r#"
        [GlueHandle(Library:"foreign", Nullable:false)] pub type Token<T> { u64 token, }
        [Export(Abi:"C",Symbol:"signed_token")] pub Token<i64> Signed(Token<i64> token) { return token; }
        [Export(Abi:"C",Symbol:"unsigned_token")] pub Token<u64> Unsigned(Token<u64> token) { return token; }
        [Export(Abi:"C",Symbol:"ordinary")] pub u64 Ordinary(u64 token) { return token; }
    "#;
    let (db,_typed,root,_)=support::input_fixture_with_source(source);
    let mut declarations=Vec::new();
    for (name,scalar) in [("Signed","i64"),("Unsigned","u64")] {
        let key=function(&db,root,name).unwrap();
        let shape=beskid_queries::glue_handle_shape(&db,key,Some(0)).unwrap().unwrap();
        let beskid_queries::DynamicPackingNode::Record{declaration,arguments,fields,..}=&shape.nodes()[0] else {
            panic!("opaque signature must retain its actual nominal record");
        };
        assert_eq!(arguments.len(),1);
        assert!(matches!(&shape.nodes()[arguments[0] as usize],beskid_queries::DynamicPackingNode::Scalar{name,..} if *name==scalar));
        assert_eq!(fields.len(),1);
        assert!(!fields[0].managed);
        assert!(matches!(&shape.nodes()[fields[0].ty as usize],beskid_queries::DynamicPackingNode::Scalar{name:"u64",managed:false}));
        declarations.push(*declaration);
        let result=beskid_queries::glue_handle_shape(&db,key,None).unwrap().unwrap();
        assert_eq!(result.nodes(),shape.nodes());
    }
    assert_eq!(declarations[0],declarations[1]);
    let scalar=function(&db,root,"Ordinary").unwrap();
    assert!(beskid_queries::glue_handle_shape(&db,scalar,Some(0)).unwrap().is_none());
}
