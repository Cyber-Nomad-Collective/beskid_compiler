//! Method calls whose callee is a member access on a receiver expression (`Make().Begin()`,
//! `(reader).Begin()`, `holder.items[0].Begin()`, fluent chains on call results, and the
//! `(this).reader.Begin()` shape Mod-generated decoders spell) lower as direct method calls,
//! including contract members reached through a contract parameter, a where-bounded generic
//! parameter, and a where-bounded generic field of a bounded impl.

use super::support::{
    AstNodeKey, CodegenInput, SyntaxModuleItem, find_nodes_of_kind, item_fixture_with_root, item_name,
    lower_syntax_program,
};

const PRELUDE: &str = r#"
enum Error { Failed() }
enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }
contract Source { Result<i64, Error> Begin(i64 limit); }
contract Decoder { Result<i64, Error> ReadTyped(i64 limit); }
type Reader : Source {
    i64 n,
    pub Result<i64, Error> Begin(i64 limit) { return Result::Ok(limit + n); }
    pub Reader With(i64 v) { return Reader { n: n + v }; }
    pub i64 Value() { return n; }
}
Reader MakeReader() { return Reader { n: 1_i64 }; }
type Holder { Reader[] items, Reader reader }
type Adapter<R> { R reader }
"#;

/// The bounded impl whose generic field receiver is spelled `receiver` (`(this).reader` is a
/// member access callee, `this.reader` one path callee).
fn adapter(receiver: &str) -> String {
    format!(
        "impl<R> Adapter<R> : Decoder where R: Source {{
    pub Result<i64, Error> ReadTyped(i64 limit) {{
        i64 value = {receiver}.Begin(limit)?;
        return Result::Ok(value);
    }}
}}
"
    )
}

const ADAPTER_MAIN: &str = r#"
i64 Unwrap(Result<i64, Error> value) { return match value { Result::Ok(v) => v, Result::Error(_) => 0_i64, }; }
i64 Main() {
    Adapter<Reader> adapter = Adapter<Reader> { reader: MakeReader() };
    return Unwrap(adapter.ReadTyped(3_i64));
}
"#;

/// Every method and free function under `root`, named by its declared name.
fn module_items(input: &CodegenInput<'_>, root: AstNodeKey) -> Vec<SyntaxModuleItem> {
    let mut items = Vec::new();
    for kind in [beskid_queries::IndexedNodeKind::MethodDefinition, beskid_queries::IndexedNodeKind::FunctionDefinition]
    {
        for key in find_nodes_of_kind(input.database(), root, kind) {
            let name = item_name(input.database(), key).ok().flatten().expect("item name");
            items.push(SyntaxModuleItem { key, symbol: name.to_string() });
        }
    }
    items
}

/// Lower every item of `PRELUDE` + `source`; on failure name each call's lowering so the
/// declining classification is visible.
fn lower(source: &str) {
    let source = format!("{PRELUDE}{}{source}", adapter("(this).reader"));
    let (input, isa, root) = item_fixture_with_root(&source);
    let db = input.database();
    for call in find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::CallExpression) {
        let lowering = beskid_queries::call_lowering(db, call);
        assert!(
            !matches!(lowering, Ok(Some(beskid_queries::CallLowering::Dynamic))),
            "no call of this source is dynamic: {} -> {lowering:?}",
            beskid_queries::format_ast_node_site(db, call)
        );
    }
    let items = module_items(&input, root);
    if let Err(error) = lower_syntax_program(&input, isa.as_ref(), &items) {
        let calls = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::CallExpression)
            .into_iter()
            .map(|call| {
                format!(
                    "{} -> {:?}",
                    beskid_queries::format_ast_node_site(db, call),
                    beskid_queries::call_lowering(db, call)
                )
            })
            .collect::<Vec<_>>();
        panic!("member method calls lower: {error:?}\n{}", calls.join("\n"));
    }
}

#[test]
fn methods_on_call_results_grouped_locals_and_indexed_fields_lower_directly() {
    lower(
        r#"
Result<i64, Error> Main(Reader reader, Holder holder) {
    i64 a = MakeReader().Begin(1_i64)?;
    i64 b = (reader).Begin(2_i64)?;
    i64 c = holder.items[0].Begin(3_i64)?;
    i64 d = (holder).reader.Begin(4_i64)?;
    return Result::Ok(a + b + c + d);
}
"#,
    );
}

#[test]
fn a_method_on_a_field_of_the_implicit_receiver_member_lowers_directly() {
    lower(
        r#"
type Owner {
    Reader reader,
    pub Result<i64, Error> Run() { i64 value = (this).reader.Begin(1_i64)?; return Result::Ok(value); }
}
Result<i64, Error> Main() { Owner owner = Owner { reader: MakeReader() }; return owner.Run(); }
"#,
    );
}

#[test]
fn fluent_chains_on_call_results_lower_directly() {
    lower("i64 Main() { return MakeReader().With(2_i64).With(3_i64).Value(); }");
}

#[test]
fn contract_and_bounded_generic_member_receivers_lower_through_their_witnesses() {
    lower(
        r#"
Result<i64, Error> ViaContract(Source source) { i64 value = (source).Begin(1_i64)?; return Result::Ok(value); }
Result<i64, Error> ViaBound<R>(R reader) where R: Source { i64 value = (reader).Begin(2_i64)?; return Result::Ok(value); }
i64 Unwrap(Result<i64, Error> value) { return match value { Result::Ok(v) => v, Result::Error(_) => 0_i64, }; }
i64 Main() {
    return Unwrap(ViaContract(MakeReader())) + Unwrap(ViaBound<Reader>(MakeReader()));
}
"#,
    );
}

#[test]
fn a_bounded_generic_field_receiver_lowers_through_the_impl_instance() {
    lower(ADAPTER_MAIN);
}

/// Control: the same bounded generic field receiver spelled as one path callee.
#[test]
fn a_bounded_generic_field_path_callee_lowers_through_the_impl_instance() {
    let source = format!("{PRELUDE}{}{ADAPTER_MAIN}", adapter("this.reader"));
    let (input, isa, root) = item_fixture_with_root(&source);
    let items = module_items(&input, root);
    lower_syntax_program(&input, isa.as_ref(), &items).expect("the path callee lowers");
}
