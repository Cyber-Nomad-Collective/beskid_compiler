//! Default field privacy: a field without `pub` is private to the source unit that declares its
//! type. Reads, projection chains, and struct literals from another unit are coded E1211 member
//! errors before lowering; the declaring unit and `pub` fields keep full access.

use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{
    MemberReferenceKind, aggregate_field_access, aggregate_literal_layout, check_items, member_reference_legality,
};

use super::deadline_projection::fact_result;

const MODEL_SOURCE: &str =
    "pub type Inner { i64 secret, pub i64 shared } pub type Request { pub Inner inner, i64 token }";

fn model(owner_source: &str) -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf, String) {
    let temp = tempfile::tempdir().expect("application root");
    let owner_path = temp.path().join("Main.bd");
    let model_path = temp.path().join("Shapes/Request.bd");
    (temp, owner_path, model_path, owner_source.to_string())
}

#[test]
fn cross_unit_read_of_a_private_field_is_an_inaccessible_member_error() {
    let (_temp, owner_path, model_path, owner_source) =
        model("use Shapes.Request; i64 Reveal(Request request) { return request.token; }");
    let finding = fact_result(
        owner_path,
        owner_source,
        model_path,
        MODEL_SOURCE.into(),
        "i64 Reveal",
        NodeKind::FunctionDefinition,
        |db, key| member_reference_legality(db, key),
    )
    .expect("member legality query")
    .expect("a private field read from another unit must be rejected before lowering");
    assert_eq!(finding.kind, MemberReferenceKind::InaccessibleStructField { name: "token".into() });
}

#[test]
fn cross_unit_read_of_a_private_field_has_no_field_access_fact() {
    let (_temp, owner_path, model_path, owner_source) =
        model("use Shapes.Request; i64 Reveal(Request request) { return request.token; }");
    let result = fact_result(
        owner_path,
        owner_source,
        model_path,
        MODEL_SOURCE.into(),
        "request.token",
        NodeKind::PathExpression,
        |db, key| aggregate_field_access(db, key),
    );
    assert!(result.is_err(), "lowering must fail closed on a private field of another unit: {result:?}");
}

#[test]
fn cross_unit_read_of_a_pub_field_is_accessible() {
    let (_temp, owner_path, model_path, owner_source) =
        model("use Shapes.Request; Inner Reveal(Request request) { return request.inner; }");
    let result = fact_result(
        owner_path,
        owner_source,
        model_path,
        MODEL_SOURCE.into(),
        "request.inner",
        NodeKind::PathExpression,
        |db, key| aggregate_field_access(db, key),
    );
    assert!(result.expect("pub field query").is_some(), "a pub field stays readable from another unit");
}

#[test]
fn same_unit_read_of_a_private_field_is_accessible() {
    let (_temp, owner_path, model_path, owner_source) =
        model("pub type Local { i64 hidden } i64 Reveal(Local local) { return local.hidden; }");
    let result = fact_result(
        owner_path,
        owner_source,
        model_path,
        MODEL_SOURCE.into(),
        "local.hidden",
        NodeKind::PathExpression,
        |db, key| aggregate_field_access(db, key),
    );
    assert!(result.expect("same-unit field query").is_some(), "the declaring unit keeps private field access");
}

#[test]
fn projection_chain_through_a_private_field_names_that_field() {
    let (_temp, owner_path, model_path, owner_source) =
        model("use Shapes.Request; i64 Reveal(Request request) { return request.inner.secret; }");
    let finding = fact_result(
        owner_path,
        owner_source,
        model_path,
        MODEL_SOURCE.into(),
        "i64 Reveal",
        NodeKind::FunctionDefinition,
        |db, key| member_reference_legality(db, key),
    )
    .expect("member legality query")
    .expect("a projection chain through a private field must be rejected before lowering");
    assert_eq!(finding.kind, MemberReferenceKind::InaccessibleStructField { name: "secret".into() });
}

#[test]
fn projection_chain_through_pub_fields_has_no_member_error() {
    let (_temp, owner_path, model_path, owner_source) =
        model("use Shapes.Request; i64 Reveal(Request request) { return request.inner.shared; }");
    let finding = fact_result(
        owner_path,
        owner_source,
        model_path,
        MODEL_SOURCE.into(),
        "i64 Reveal",
        NodeKind::FunctionDefinition,
        |db, key| member_reference_legality(db, key),
    )
    .expect("member legality query");
    assert!(finding.is_none(), "a chain of pub fields is accessible: {finding:?}");
}

#[test]
fn cross_unit_literal_with_a_private_field_is_an_inaccessible_member_error() {
    let (_temp, owner_path, model_path, owner_source) =
        model("use Shapes.Request; Inner Forge() { return Inner { secret: 1_i64, shared: 2_i64 }; }");
    let finding = fact_result(
        owner_path.clone(),
        owner_source.clone(),
        model_path.clone(),
        MODEL_SOURCE.into(),
        "Inner Forge",
        NodeKind::FunctionDefinition,
        |db, key| member_reference_legality(db, key),
    )
    .expect("member legality query")
    .expect("a literal supplying a private field of another unit must be rejected");
    assert_eq!(finding.kind, MemberReferenceKind::InaccessibleStructField { name: "secret".into() });
    assert!(
        fact_result(
            owner_path,
            owner_source,
            model_path,
            MODEL_SOURCE.into(),
            "Inner { secret",
            NodeKind::StructLiteralExpression,
            |db, key| aggregate_literal_layout(db, key),
        )
        .is_err(),
        "lowering must fail closed on a literal of a type with a private field from another unit"
    );
}

#[test]
fn inaccessible_field_diagnostic_is_e1211_with_pub_help() {
    let (_temp, owner_path, model_path, owner_source) =
        model("use Shapes.Request; i64 Reveal(Request request) { return request.token; }");
    let findings = fact_result(
        owner_path,
        owner_source,
        model_path,
        MODEL_SOURCE.into(),
        "i64 Reveal",
        NodeKind::FunctionDefinition,
        |db, key| Ok::<_, beskid_queries::SemanticError>(check_items(db, &[key]).err()),
    )
    .expect("legality query")
    .expect("a private field read from another unit must have a legality finding");
    let finding = findings
        .iter()
        .find(|finding| finding.kind.code() == "E1211")
        .unwrap_or_else(|| panic!("expected E1211, got {findings:?}"));
    assert!(finding.kind.message().contains("inaccessible struct field `token`"), "{:?}", finding.kind.message());
    let help = finding.kind.help().expect("inaccessible field help");
    assert!(help.contains("mark the field `pub` in its declaring type"), "{help}");
}
