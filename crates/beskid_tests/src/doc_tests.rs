use beskid_analysis::doc_comment_parser::DocSyntaxParser;
use beskid_analysis::doc_comment_parser::Rule as DocRule;
use beskid_analysis::services::{build_document_analysis, hover_at_offset, parse_program};
use beskid_analysis::{BeskidParser, Rule as MainRule};
use pest::Parser;

#[test]
fn triple_slash_doc_normalized_on_program_item() {
    let src = "/// Summary line\n/// Second line\nunit main() { return 42; }\n";
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
fn hover_includes_doc_markdown_when_resolved() {
    let src = "/// Hello **doc**\nunit main() { return 42; }\n";
    let program = parse_program(src).unwrap();
    let snap = build_document_analysis(&program);
    let name_start = src.find("main").expect("main");
    let hover = hover_at_offset(&snap, name_start).expect("hover");
    assert!(hover.markdown.contains("Hello"));
    assert!(hover.markdown.contains("**doc**") || hover.markdown.contains("doc"));
}

#[test]
fn program_rule_accepts_single_function_with_trailing_newline() {
    let src = "unit main() { return 42; }\n";
    BeskidParser::parse(MainRule::Program, src).expect("program parser");
}

#[test]
fn program_rule_accepts_doc_and_function_with_trailing_newline() {
    let src = "/// Hello\nunit main() { return 42; }\n";
    BeskidParser::parse(MainRule::Program, src).expect("program parser");
}

#[test]
fn item_with_docs_rule_accepts_doc_and_function_with_trailing_newline() {
    let src = "/// Hello\nunit main() { return 42; }\n";
    BeskidParser::parse(MainRule::ItemWithDocs, src).expect("item parser");
}

#[test]
fn item_with_docs_rule_accepts_doc_and_function_without_trailing_newline() {
    let src = "/// Hello\nunit main() { return 42; }";
    BeskidParser::parse(MainRule::ItemWithDocs, src).expect("item parser");
}

#[test]
fn inner_item_rule_accepts_function_with_trailing_newline() {
    let src = "unit main() { return 42; }\n";
    BeskidParser::parse(MainRule::InnerItem, src).expect("inner item parser");
}
