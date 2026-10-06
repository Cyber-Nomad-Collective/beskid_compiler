//! Ownership geometry must come from registered source and its allocation descriptor.
#[path = "codegen_input/support.rs"]
#[allow(unused_imports, dead_code)]
mod support;

use beskid_abi::abi_v5::AbiManifestV5;
use beskid_codegen::CodegenInput;
use beskid_queries::{AstNodeKey, IndexedNodeKind, child_nodes, node_kind};
use std::sync::Arc;

const SOURCE: &str = r#"
pub type GlueOwnerRecord {
    pub GlueOwnerRecord[] next,
    pub u8[] payload,
    pub pointer foreignBuffer,
    pub word length,
    pub word allocationSize,
    pub u64 processIssuer,
    pub u64 session,
    pub u64 library,
    pub u64 generation,
    pub u64 kind,
    pub u64 shape,
    pub u64 token,
    pub u64 released,
}
pub GlueOwnerRecord Construct(GlueOwnerRecord[] next, u8[] payload, pointer foreignBuffer,
    word length, word allocationSize, u64 processIssuer, u64 session, u64 library,
    u64 generation, u64 kind, u64 shape, u64 token) {
    return GlueOwnerRecord { next: next, payload: payload, foreignBuffer: foreignBuffer,
        length: length, allocationSize: allocationSize, processIssuer: processIssuer,
        session: session, library: library, generation: generation, kind: kind,
        shape: shape, token: token, released: 0 };
}
"#;

#[test]
fn registered_owner_record_constructor_owns_its_traced_descriptor_geometry() {
    assert_geometry(SOURCE, 120, &[16, 24], &(16..120).step_by(8).collect::<Vec<_>>());
}

#[test]
fn ordinary_mixed_record_traces_managed_fields_but_not_native_pointer() {
    assert_geometry(
        r#"
        type Leaf { i64 value }
        type Mixed { pointer native, u8[] bytes, Leaf leaf, word scalar }
        Mixed Construct(pointer native, u8[] bytes, Leaf leaf, word scalar) {
            return Mixed { native: native, bytes: bytes, leaf: leaf, scalar: scalar };
        }
    "#,
        48,
        &[24, 32],
        &[16, 24, 32, 40],
    );
}

#[test]
fn native_pointer_only_record_has_no_gc_trace_slots() {
    assert_geometry(
        r#"
        type Native { pointer address, u64 tag }
        Native Construct(pointer address, u64 tag) {
            return Native { address: address, tag: tag };
        }
    "#,
        32,
        &[],
        &[16, 24],
    );
}

#[test]
fn ordinary_utf8_geometry_traces_only_its_managed_backing_array() {
    assert_geometry(
        r#"
        type Utf8ViewRecord { u8[] payload, pointer data, word length }
        Utf8ViewRecord Construct(u8[] payload, pointer data, word length) {
            return Utf8ViewRecord { payload: payload, data: data, length: length };
        }
    "#,
        40,
        &[16],
        &[16, 24, 32],
    );
}

#[test]
fn ordinary_utf8_lookalike_cannot_issue_canonical_view_descriptor() {
    let source = r#"
        type Utf8ViewRecord { u8[] payload, pointer data, word length }
        Utf8ViewRecord Construct(u8[] payload, pointer data, word length) {
            return Utf8ViewRecord { payload: payload, data: data, length: length };
        }
    "#;
    let (db, typed, root, target) = support::input_fixture_with_source(source);
    let input =
        CodegenInput::new(&db, typed, Arc::from([root]), target.clone(), AbiManifestV5::canonical_runtime(target))
            .expect("registered ordinary source");
    fn literal(db: &dyn beskid_queries::Db, key: AstNodeKey) -> Option<AstNodeKey> {
        if node_kind(db, key).ok().flatten() == Some(IndexedNodeKind::StructLiteralExpression) {
            return Some(key);
        }
        child_nodes(db, key).ok().flatten()?.iter().find_map(|child| literal(db, *child))
    }
    let plan =
        input.aggregate_static_plan(literal(&db, root).expect("typed constructor")).expect("ordinary aggregate plan");
    assert_eq!(plan.descriptor_flags, 0, "shape and name do not grant canonical String authority");
}

#[test]
fn ordinary_dynamic_names_do_not_issue_managed_erasure_authority() {
    let source = "pub type DynamicValueV1 {} pub type DynamicPayloadV1 {}";
    let (db, typed, root, target) = support::input_fixture_with_source(source);
    let input = CodegenInput::new(&db, typed, Arc::from([root]), target.clone(),
        AbiManifestV5::canonical_runtime(target)).expect("registered untrusted lookalikes");
    fn declarations(db: &dyn beskid_queries::Db, key: AstNodeKey, out: &mut Vec<AstNodeKey>) {
        if node_kind(db, key).expect("declaration kind") == Some(IndexedNodeKind::TypeDefinition) {
            out.push(key);
            return;
        }
        for child in child_nodes(db, key).expect("declaration children").unwrap_or_default().iter().copied() {
            declarations(db, child, out);
        }
    }
    let mut declarations_found = Vec::new();
    declarations(&db, root, &mut declarations_found);
    let declarations = declarations_found;
    assert_eq!(declarations.len(), 2);
    for declaration in declarations {
        assert_eq!(beskid_queries::runtime_managed_opaque_kind(&db, declaration), None,
            "a name alone must never authorize the canonical 40-byte erased cell");
        assert_eq!(input.aggregate_object_layout(declaration).expect("ordinary layout").object_size, 16,
            "untrusted empty types remain ordinary header-only records");
    }
}

fn assert_geometry(source: &str, size: u64, trace: &[u64], offsets: &[u64]) {
    let (db, typed, root, target) = support::input_fixture_with_source(source);
    let input =
        CodegenInput::new(&db, typed, Arc::from([root]), target.clone(), AbiManifestV5::canonical_runtime(target))
            .expect("registered owner source");
    fn literals(db: &dyn beskid_queries::Db, key: AstNodeKey, out: &mut Vec<AstNodeKey>) {
        if node_kind(db, key).expect("kind") == Some(IndexedNodeKind::StructLiteralExpression) {
            out.push(key);
        }
        for child in child_nodes(db, key).expect("children").unwrap_or_default().iter().copied() {
            literals(db, child, out);
        }
    }
    let mut found = Vec::new();
    literals(&db, root, &mut found);
    assert_eq!(found.len(), 1, "real typed owner constructor literal");
    let plan = input.aggregate_static_plan(found[0]).expect("canonical owner allocation plan");
    assert_eq!(plan.object_size, size);
    assert_eq!(plan.object_alignment, 8);
    assert_eq!(
        plan.pointer_map_offsets.as_ref(),
        trace,
        "managed array/nominal slots trace; native pointer slots never trace"
    );
    assert_eq!(plan.fields.iter().map(|field| field.field_offset).collect::<Vec<_>>(), offsets);
    assert!(!plan.descriptor_symbol.is_empty());
    assert!(!plan.pointer_map_symbol.is_empty());
    assert!(!plan.allocation_request_symbol.is_empty());
}
