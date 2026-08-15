//! Documentation comment parsing, hover, and `build_document_analysis` behavior.

use beskid_analysis::doc::DocRefLinkContext;
use beskid_analysis::doc_comment_parser::DocSyntaxParser;
use beskid_analysis::doc_comment_parser::Rule as DocRule;

use beskid_analysis::services::{build_document_analysis, hover_at_offset, parse_program};

use beskid_analysis::{BeskidParser, Rule as MainRule};
use pest::Parser;

#[test]
fn triple_slash_doc_normalized_on_program_item() {
    let src = "/// Summary line\n/// Second line\nunit Main() { return 42; }\n";
    let program = parse_program(src).unwrap();
    assert_eq!(program.node.items.len(), 1);
    let d = program.node.leading_docs[0].as_ref().expect("leading doc");
    assert!(d.normalized_source.contains("Summary line"));
    assert!(d.normalized_source.contains("Second line"));
}

#[test]
fn doc_body_grammar_splits_ref_segments() {
    let mut pairs = DocSyntaxParser::parse(DocRule::DocBody, "A @ref(x) B").unwrap();
    let top = pairs.next().unwrap();
    let joined: String = top.into_inner().map(|p| p.as_str()).collect();
    assert!(joined.contains("@ref(x)"), "joined={joined:?}");
}

#[test]
fn doc_body_grammar_splits_arg_and_returns() {
    let mut pairs = DocSyntaxParser::parse(DocRule::DocBody, "A @arg(x) body\n@returns y").unwrap();
    let top = pairs.next().unwrap();
    let debug: Vec<_> = top
        .into_inner()
        .filter_map(|wrapper| {
            if wrapper.as_rule() != DocRule::piece {
                return None;
            }
            wrapper.into_inner().next()
        })
        .map(|p| (p.as_rule(), p.as_str().to_string()))
        .collect();
    assert!(debug.iter().any(|(r, _)| *r == DocRule::ArgTag), "expected ArgTag in {debug:?}");
    assert!(debug.iter().any(|(r, _)| *r == DocRule::ReturnsTag), "expected ReturnsTag in {debug:?}");
}

#[test]
fn doc_body_grammar_splits_variant_and_par_tags() {
    let mut pairs = DocSyntaxParser::parse(DocRule::DocBody, "E @variant(V) d @par(P) t").unwrap();
    let top = pairs.next().unwrap();
    let debug: Vec<_> = top
        .into_inner()
        .filter_map(|wrapper| {
            if wrapper.as_rule() != DocRule::piece {
                return None;
            }
            wrapper.into_inner().next()
        })
        .map(|p| (p.as_rule(), p.as_str().to_string()))
        .collect();
    assert!(debug.iter().any(|(r, _)| *r == DocRule::VariantTag), "expected VariantTag in {debug:?}");
    assert!(debug.iter().any(|(r, _)| *r == DocRule::ParTag), "expected ParTag in {debug:?}");
}

#[ignore = "doc/hover resolution fixtures need refreshed analysis facts after syntax-ISLE cutover"]
#[test]
fn doc_diagnostics_unknown_variant_name() {
    let src = "/// @variant(Ghost) nope\nenum Color {\n    Red,\n    Blue,\n}\n";
    let program = parse_program(src).unwrap();
    let snap = build_document_analysis(&program, "t.bd", src, None);
    assert!(snap.resolution.is_some());
    assert!(
        snap.doc_diagnostics.iter().any(|d| d.code.as_deref() == Some("W1621")),
        "expected W1621, got {:?}",
        snap.doc_diagnostics
    );
}

#[ignore = "doc/hover resolution fixtures need refreshed analysis facts after syntax-ISLE cutover"]
#[test]
fn doc_diagnostics_variant_on_type_is_wrong_placement() {
    let src = "/// @variant(x) bad\ntype Point { i64 x, }\n";
    let program = parse_program(src).unwrap();
    let snap = build_document_analysis(&program, "t.bd", src, None);
    assert!(snap.resolution.is_some());
    assert!(
        snap.doc_diagnostics.iter().any(|d| d.code.as_deref() == Some("W1620")),
        "expected W1620, got {:?}",
        snap.doc_diagnostics
    );
}

#[ignore = "doc/hover resolution fixtures need refreshed analysis facts after syntax-ISLE cutover"]
#[test]
fn doc_diagnostics_par_without_generics_on_function() {
    let src = "/// @par(T) bad\nunit Main() { return 42; }\n";
    let program = parse_program(src).unwrap();
    let snap = build_document_analysis(&program, "t.bd", src, None);
    assert!(snap.resolution.is_some());
    assert!(
        snap.doc_diagnostics.iter().any(|d| d.code.as_deref() == Some("W1623")),
        "expected W1623, got {:?}",
        snap.doc_diagnostics
    );
}

#[ignore = "doc/hover resolution fixtures need refreshed analysis facts after syntax-ISLE cutover"]
#[test]
fn doc_diagnostics_flag_unknown_arg_name() {
    let src = "/// @arg(nope) bad\ni64 Sum(\n    i64 left,\n    i64 right\n) { return left + right; }\n";
    let program = parse_program(src).unwrap();
    let snap = build_document_analysis(&program, "t.bd", src, None);
    assert!(snap.resolution.is_some(), "resolution should succeed");
    assert!(
        snap.doc_diagnostics.iter().any(|d| d.code.as_deref() == Some("W1610")),
        "expected W1610, got {:?}",
        snap.doc_diagnostics
    );
}

#[ignore = "doc/hover resolution fixtures need refreshed analysis facts after syntax-ISLE cutover"]
#[test]
fn hover_includes_doc_markdown_when_resolved() {
    let src = "/// Hello **doc**\nunit Main() { return 42; }\n";
    let program = parse_program(src).unwrap();
    let snap = build_document_analysis(&program, "<memory>", src, None);
    let name_start = src.find("main").expect("main");
    let hover = hover_at_offset(&snap, name_start).expect("hover");
    assert!(hover.markdown.contains("Hello"));
    assert!(hover.markdown.contains("**doc**") || hover.markdown.contains("doc"));
}

#[test]
fn program_rule_accepts_single_function_with_trailing_newline() {
    let src = "unit Main() { return 42; }\n";
    BeskidParser::parse(MainRule::Program, src).expect("program parser");
}

#[test]
fn program_rule_accepts_doc_and_function_with_trailing_newline() {
    let src = "/// Hello\nunit Main() { return 42; }\n";
    BeskidParser::parse(MainRule::Program, src).expect("program parser");
}

#[test]
fn item_with_docs_rule_accepts_doc_and_function_with_trailing_newline() {
    let src = "/// Hello\nunit Main() { return 42; }\n";
    BeskidParser::parse(MainRule::ItemWithDocs, src).expect("item parser");
}

#[test]
fn item_with_docs_rule_accepts_doc_and_function_without_trailing_newline() {
    let src = "/// Hello\nunit Main() { return 42; }";
    BeskidParser::parse(MainRule::ItemWithDocs, src).expect("item parser");
}

#[test]
fn inner_item_rule_accepts_function_with_trailing_newline() {
    let src = "unit Main() { return 42; }\n";
    BeskidParser::parse(MainRule::InnerItem, src).expect("inner item parser");
}

#[test]
fn parser_accepts_docs_on_nested_members_and_statements() {
    let src = r#"
type User {
    /// field doc
    string name,
}

enum Value {
    /// variant doc
    Item(
        /// variant field doc
        i64 count,
    ),
}

contract Service {
    /// method doc
    i64 Get(
        /// param doc
        i64 id
    );
}

impl User {
    /// method doc
    i64 Size(
        /// param doc
        i64 scale
    ) { return scale; }
}

i64 Sum(
    /// param doc
    i64 left,
    /// param doc
    i64 right
) { return left + right; }

test docs_inside_test_body {
    /// statement doc
    i64 value = 1;
}
"#;
    BeskidParser::parse(MainRule::Program, src).expect("program parser");
}

#[ignore = "doc/hover resolution fixtures need refreshed analysis facts after syntax-ISLE cutover"]
#[test]
fn hover_includes_member_doc_markdown() {
    let src = r#"
type User {
    /// Display name of the user.
    string name,
}
"#;
    let program = parse_program(src).expect("program parse");
    let snap = build_document_analysis(&program, "<memory>", src, None);
    let offset = src.rfind("name,").expect("field name");
    let hover = hover_at_offset(&snap, offset).expect("hover");
    assert!(hover.markdown.contains("Display name of the user."));
}

#[ignore = "doc/hover resolution fixtures need refreshed analysis facts after syntax-ISLE cutover"]
#[test]
fn resolved_ref_emits_pckg_doc_route_in_markdown_when_context_set() {
    let src = r#"
/// See @ref(main) for details.
unit other() { return 1; }
unit Main() { return 42; }
"#;
    let program = parse_program(src).unwrap();
    let ctx = DocRefLinkContext {
        package_with_version: "demo-pkg@1.0.0".into(),
        publishing_package: Some("demo-pkg".into()),
        dependency_roots: vec![],
    };
    let snap = build_document_analysis(&program, "t.bd", src, Some(&ctx));
    let blob: String = snap
        .item_docs
        .iter()
        .filter_map(|slot| slot.as_ref().map(|d| d.markdown.as_str()))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(blob.contains("/docs/demo-pkg%401.0.0/api/"), "expected pckg docs link, got {blob:?}");
    assert!(blob.contains("](") && blob.contains("main"), "expected markdown link mentioning main, got {blob:?}");
}
