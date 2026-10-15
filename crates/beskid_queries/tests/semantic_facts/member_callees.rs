//! Calls whose callee is a member access on a receiver expression (`Make().Begin()`,
//! `(reader).Begin()`, `holder.items[0].Begin()`, `xs.With(1).Value()`) rather than one path
//! callee. The callee member is classified by what it resolves to on the receiver's type: a
//! nominal, impl, contract or where-bounded method is a method call, and only a function-typed
//! field is a function value call. The same receiver forms are what Mod-generated syntax spells
//! (`Member(Member(Local("this"), "reader"), "Begin")`).

use beskid_analysis::macros::{DEFAULT_MAX_MACRO_EXPANSION_DEPTH, expand_program};
use beskid_analysis::services::parse_program;
use beskid_analysis::syntax::{CallExpression, Expression};
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{AstNodeKey, CallLowering, call_lowering, check_items, try_expression_fact};

use super::support::setup;

const PRELUDE: &str = "enum Error { Failed() } \
    enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } \
    contract Source { Result<i64, Error> Begin(i64 limit); } \
    type Reader : Source { \
        i64 n, \
        pub Result<i64, Error> Begin(i64 limit) { return Result::Ok(limit + n); } \
        pub Reader With(i64 v) { return Reader { n: n + v }; } \
        pub i64 Value() { return n; } \
    } \
    Reader MakeReader() { return Reader { n: 1 }; } \
    type Holder { Reader[] items, Reader reader } ";

/// Every call whose callee is a member access named `member`, as node keys.
fn member_calls(source: &str, member: &str) -> Vec<AstNodeKey> {
    let (_, _, unit, generation, index) = setup(source);
    let program = expand_program(parse_program(source).expect("parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    index
        .ids_of_kind(NodeKind::CallExpression)
        .filter(|node| {
            index.node_at(&program, *node).and_then(|node| node.of::<CallExpression>()).is_some_and(|call| {
                matches!(&call.callee.node, Expression::Member(callee) if callee.node.member.node.name == member)
            })
        })
        .map(|node| AstNodeKey { unit, generation, node })
        .collect()
}

/// Every member call named in `members` is a method call (Direct to a method or contract method
/// signature), every try expression has its Result fact, and the legality gate reports nothing.
fn assert_member_methods(source: &str, members: &[&str]) {
    let source = format!("{PRELUDE}{source}");
    let (db, _project, unit, generation, index) = setup(&source);
    for member in members {
        let calls = member_calls(&source, member);
        assert!(!calls.is_empty(), "the source spells `{member}` as a member access callee");
        for call in calls {
            let lowering = call_lowering(&db, call).expect("member call lowering query").expect("member call lowering");
            let CallLowering::Direct(method) = lowering else {
                panic!("`.{member}(...)` must be a method call, got {lowering:?}")
            };
            assert!(
                matches!(index.kind(method.node), Some(NodeKind::MethodDefinition | NodeKind::ContractMethodSignature)),
                "`.{member}(...)` resolves to {:?}",
                index.kind(method.node)
            );
        }
    }
    for node in index.ids_of_kind(NodeKind::TryExpression) {
        let expression = AstNodeKey { unit, generation, node };
        let fact = try_expression_fact(&db, expression).expect("try expression query");
        assert!(fact.is_some(), "a try on a member method call has a Result fact");
    }
    let items = index
        .ids_of_kind(NodeKind::FunctionDefinition)
        .chain(index.ids_of_kind(NodeKind::MethodDefinition))
        .map(|node| AstNodeKey { unit, generation, node })
        .collect::<Vec<_>>();
    if let Err(findings) = check_items(&db, &items) {
        let codes =
            findings.iter().map(|finding| format!("{} {:?}", finding.kind.code(), finding.kind)).collect::<Vec<_>>();
        panic!("member method calls pass the legality gate: {codes:#?}");
    }
}

#[test]
fn a_method_called_on_a_call_result_is_a_method_call() {
    assert_member_methods(
        "Result<i64, Error> Run() { i64 value = MakeReader().Begin(1)?; return Result::Ok(value); }",
        &["Begin"],
    );
}

#[test]
fn a_method_called_on_a_grouped_local_is_a_method_call() {
    assert_member_methods(
        "Result<i64, Error> Run(Reader reader) { i64 value = (reader).Begin(1)?; return Result::Ok(value); }",
        &["Begin"],
    );
}

#[test]
fn a_method_called_on_an_indexed_field_is_a_method_call() {
    assert_member_methods(
        "Result<i64, Error> Run(Holder holder) { i64 value = holder.items[0].Begin(1)?; return Result::Ok(value); }",
        &["Begin"],
    );
}

#[test]
fn a_method_called_on_a_field_of_a_grouped_receiver_is_a_method_call() {
    assert_member_methods(
        "Result<i64, Error> Run(Holder holder) { i64 value = (holder).reader.Begin(1)?; return Result::Ok(value); }",
        &["Begin"],
    );
}

#[test]
fn a_method_called_on_a_field_of_the_implicit_receiver_member_is_a_method_call() {
    assert_member_methods(
        "type Owner { Reader reader, \
             pub Result<i64, Error> Run() { i64 value = (this).reader.Begin(1)?; return Result::Ok(value); } }",
        &["Begin"],
    );
}

#[test]
fn a_fluent_chain_on_call_results_is_a_chain_of_method_calls() {
    assert_member_methods("i64 Run() { return MakeReader().With(2).With(3).Value(); }", &["With", "Value"]);
}

#[test]
fn a_contract_member_called_on_a_grouped_contract_parameter_is_a_method_call() {
    assert_member_methods(
        "Result<i64, Error> Run(Source source) { i64 value = (source).Begin(1)?; return Result::Ok(value); }",
        &["Begin"],
    );
}

#[test]
fn a_bound_member_called_on_a_grouped_generic_parameter_is_a_method_call() {
    assert_member_methods(
        "Result<i64, Error> Run<R>(R reader) where R: Source { i64 value = (reader).Begin(1)?; return Result::Ok(value); }",
        &["Begin"],
    );
}

#[test]
fn a_bound_member_called_on_a_generic_field_of_the_implicit_receiver_is_a_method_call() {
    assert_member_methods(
        "contract Decoder { Result<i64, Error> ReadTyped(i64 limit); } \
         type Adapter<R> { R reader } \
         impl<R> Adapter<R> : Decoder where R: Source { \
             pub Result<i64, Error> ReadTyped(i64 limit) { i64 value = (this).reader.Begin(limit)?; return Result::Ok(value); } \
         }",
        &["Begin"],
    );
}

#[test]
fn a_function_typed_field_of_a_member_receiver_stays_a_function_value_call() {
    let source = format!(
        "{PRELUDE}type Handler {{ (i64) => i64 op }} \
         Handler MakeHandler() {{ return Handler {{ op: (i64 x) => x + 1 }}; }} \
         i64 Run(Handler handler) {{ return MakeHandler().op(1) + (handler).op(2); }}"
    );
    let (db, _project, _unit, _generation, _index) = setup(&source);
    let calls = member_calls(&source, "op");
    assert_eq!(calls.len(), 2);
    for call in calls {
        let lowering = call_lowering(&db, call).expect("field call lowering query").expect("field call lowering");
        assert!(matches!(lowering, CallLowering::FunctionValue(_)), "{lowering:?}");
    }
}
