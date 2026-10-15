//! Source identity of array literals and unit calls: the shapes a native Mod body using the
//! compiler-SDK emitter types (`SpannedIdentifierList { items: [] }`, `["Native.GeneratedUnit"]`,
//! `Contribution.From([Items.FunctionItem(...)])`) asks `source_expression_type` for.

use super::*;
use crate::{BeskidDatabase, build_typed_program, project_session_for_planned_syntax_assembly};
use beskid_analysis::projects::{
    AssemblyOptions, CompilePlan, ProgramAssembly, Target, TargetKind, assemble_program_with_materializer,
};
use beskid_analysis::syntax_query::NodeKind;

fn fixture(source: &str) -> (tempfile::TempDir, BeskidDatabase, Arc<ProgramAssembly>) {
    let root = tempfile::tempdir().unwrap();
    let src = root.path().join("Src");
    std::fs::create_dir(&src).unwrap();
    let path = src.join("Main.bd");
    std::fs::write(&path, source).unwrap();
    let plan = CompilePlan {
        project_root: root.path().into(),
        manifest_path: root.path().join("Host.bproj"),
        project_name: "Host".into(),
        source_root: src,
        target: Target { name: "Main".into(), kind: TargetKind::Lib, entry: Some("Main.bd".into()) },
        dependency_projects: vec![],
        unresolved_dependencies: vec![],
        has_core_dependency: false,
    };
    let assembly = Arc::new(
        assemble_program_with_materializer(&plan, None, &path, None, &AssemblyOptions::default(), None, None).unwrap(),
    );
    let mut db = BeskidDatabase::default();
    let project =
        project_session_for_planned_syntax_assembly(&mut db, &assembly, &plan, &path, "array-literal-identity".into())
            .unwrap();
    build_typed_program(&mut db, project, assembly.generation, assembly.clone()).unwrap();
    (root, db, assembly)
}

fn keys(db: &BeskidDatabase, assembly: &ProgramAssembly, kind: NodeKind) -> Vec<AstNodeKey> {
    let unit = SourceUnitId::new(db, assembly.entry_unit().path.clone());
    assembly
        .entry_syntax_index()
        .ids_of_kind(kind)
        .map(|node| AstNodeKey { unit, generation: assembly.generation, node })
        .collect()
}

fn single_array_identity(source: &str) -> Result<GenericSourceTypeIdentity, SemanticError> {
    let (_root, db, assembly) = fixture(source);
    let arrays = keys(&db, &assembly, NodeKind::ArrayLiteralExpression);
    assert_eq!(arrays.len(), 1, "fixture must contain exactly one array literal: {source}");
    generic_source_expression_identity(&db, arrays[0])
}

fn strings() -> GenericSourceTypeIdentity {
    GenericSourceTypeIdentity::Array(Box::new(GenericSourceTypeIdentity::Abi(SemanticTypeId::STRING)))
}

fn nominal_array_named(identity: &GenericSourceTypeIdentity, name: &str) -> bool {
    matches!(identity, GenericSourceTypeIdentity::Array(element)
        if matches!(element.as_ref(), GenericSourceTypeIdentity::Nominal { qualified_name, arguments }
            if qualified_name.ends_with(name) && arguments.is_empty()))
}

#[test]
fn empty_literal_in_struct_field_takes_the_declared_field_type() {
    let identity = single_array_identity(
        "pub type Bag { pub string[] items, }\npub Bag Make() { return Bag { items: [] }; }",
    );
    assert_eq!(identity, Ok(strings()));
}

#[test]
fn string_literal_in_struct_field_matches_sdk_target_set_shape() {
    let identity = single_array_identity(
        "pub type TargetSet { pub string[] targetIds, }\n\
         pub TargetSet Make() { return TargetSet { targetIds: [\"Native.GeneratedUnit\"] }; }",
    );
    assert_eq!(identity, Ok(strings()));
}

#[test]
fn generic_struct_field_substitutes_spelled_type_arguments() {
    let identity = single_array_identity(
        "pub type Holder<T> { pub T[] items, }\npub Holder<i64> Make() { return Holder<i64> { items: [] }; }",
    );
    assert_eq!(
        identity,
        Ok(GenericSourceTypeIdentity::Array(Box::new(GenericSourceTypeIdentity::Abi(SemanticTypeId::I64))))
    );
}

#[test]
fn nested_nominal_elements_in_struct_field_keep_nominal_identity() {
    let identity = single_array_identity(
        "pub type Segment { pub string name, }\npub type SegmentList { pub Segment[] items, }\n\
         pub SegmentList Make(string name) { return SegmentList { items: [Segment { name: name }] }; }",
    )
    .expect("nominal element array identity");
    assert!(nominal_array_named(&identity, "Segment"), "{identity:?}");
}

#[test]
fn call_argument_literal_derives_identity_from_equal_elements() {
    let identity = single_array_identity(
        "pub i64 Count(string[] items) { return 0; }\npub i64 Main() { return Count([\"a\", \"b\"]); }",
    );
    assert_eq!(identity, Ok(strings()));
}

#[test]
fn call_argument_literal_of_struct_literals_and_calls_is_nominal() {
    let identity = single_array_identity(
        "pub type Item { pub i64 value, }\npub Item MakeItem(i64 value) { return Item { value: value }; }\n\
         pub i64 Take(Item[] items) { return 0; }\n\
         pub i64 Main() { return Take([Item { value: 1 }, MakeItem(2)]); }",
    )
    .expect("element-derived nominal array identity");
    assert!(nominal_array_named(&identity, "Item"), "{identity:?}");
}

#[test]
fn annotated_let_and_non_generic_return_supply_the_declared_type() {
    assert_eq!(single_array_identity("pub i64 Main() { string[] names = []; return 0; }"), Ok(strings()));
    assert_eq!(single_array_identity("pub string[] Names() { return []; }"), Ok(strings()));
}

#[test]
fn empty_literal_without_declared_position_fails_closed_with_its_site() {
    let error =
        single_array_identity("pub i64 Count(string[] items) { return 0; }\npub i64 Main() { return Count([]); }")
            .expect_err("an empty argument literal has no element authority here");
    assert_eq!(error.unavailable_query(), Some("source_expression_type"));
    assert!(error.unavailable_site().is_some());
    let message = error.to_string();
    assert!(message.contains("ArrayLiteralExpression@"), "{message}");
    assert!(message.contains("Main.bd"), "{message}");
    assert!(message.contains("empty array literal"), "{message}");
}

#[test]
fn generic_return_position_stays_unavailable() {
    let error = single_array_identity("pub T[] Empty<T>() { return []; }")
        .expect_err("a generic return type is interpreted only by a specialization");
    assert_eq!(error.unavailable_query(), Some("source_expression_type"));
}

#[test]
fn unit_function_call_has_unit_identity() {
    let (_root, db, assembly) = fixture("pub unit Ping() { return; }\npub unit Main() { Ping(); return; }");
    let calls = keys(&db, &assembly, NodeKind::CallExpression);
    assert_eq!(calls.len(), 1);
    assert_eq!(
        generic_source_expression_identity(&db, calls[0]),
        Ok(GenericSourceTypeIdentity::Abi(SemanticTypeId::UNIT))
    );
}

#[test]
fn unsuffixed_numeric_elements_without_declared_position_fail_closed() {
    let error = single_array_identity("pub i64 Take(u8[] bytes) { return 0; }\npub i64 Main() { return Take([1, 2]); }")
        .expect_err("an unsuffixed literal must not fix the element width to its default");
    assert_eq!(error.unavailable_query(), Some("source_expression_type"));
    assert!(error.to_string().contains("unsuffixed numeric array element"), "{error}");
}

#[test]
fn suffixed_numeric_elements_and_declared_byte_fields_are_exact() {
    let bytes = GenericSourceTypeIdentity::Array(Box::new(GenericSourceTypeIdentity::Abi(SemanticTypeId::U8)));
    assert_eq!(
        single_array_identity("pub i64 Take(u8[] bytes) { return 0; }\npub i64 Main() { return Take([1_u8, 2_u8]); }"),
        Ok(bytes.clone())
    );
    assert_eq!(
        single_array_identity("pub type Blob { pub u8[] bytes, }\npub Blob Make() { return Blob { bytes: [1, 2] }; }"),
        Ok(bytes)
    );
}
