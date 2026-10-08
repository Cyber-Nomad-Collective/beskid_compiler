//! Nominal projection chains of three and four segments (`request.context.compilation.root`,
//! the Serialization Mod's `Driver.Generate` caller) must lower to the same field loads as the
//! shorter chain they extend, one hop deeper.
use super::support::{emit_isle_item, find_function_definition, item_fixture_with_root};

const TYPES: &str = "type Node { u64 issuer, u64 gen } type Compilation { string name, Node root } type Context { Compilation compilation } type Request { Context context } ";

fn offsets_in(clif: &str) -> Vec<String> {
    clif.lines()
        .filter(|line| line.contains("load.i64"))
        .map(|line| line.split('=').nth(1).unwrap_or(line).trim().to_owned())
        .map(|instruction| instruction.rsplit(',').next().unwrap_or("").trim().to_owned())
        .map(|operand| match operand.split_once('+') {
            Some((_, offset)) => offset.to_owned(),
            None => "0".to_owned(),
        })
        .collect()
}

fn load_offsets(source: &str) -> Vec<String> {
    let (input, isa, root) = item_fixture_with_root(source);
    let item = find_function_definition(input.database(), root).expect("Main");
    let clif = emit_isle_item(&input, isa.as_ref(), item).expect("projection chain lowers").display().to_string();
    offsets_in(&clif)
}

/// Lower every function of `source` as one module and return `Main`'s CLIF.
fn main_clif_of_program(source: &str) -> String {
    use super::support::{SyntaxModuleItem, find_function_definitions, lower_syntax_program};
    let (input, isa, root) = item_fixture_with_root(source);
    let items = find_function_definitions(input.database(), root)
        .into_iter()
        .map(|key| SyntaxModuleItem {
            key,
            symbol: beskid_queries::item_name(input.database(), key).ok().flatten().expect("item name").to_string(),
        })
        .collect::<Vec<_>>();
    let artifact = lower_syntax_program(&input, isa.as_ref(), &items).expect("program lowers");
    artifact.functions.iter().find(|function| function.name == "Main").expect("Main lowered").function.display().to_string()
}

#[test]
fn four_segment_projection_extends_the_three_segment_chain_by_one_load() {
    let three = load_offsets(&format!(
        "{TYPES}u64 Main(Context context) {{ Node caller = context.compilation.root; return caller.gen; }}"
    ));
    let four = load_offsets(&format!(
        "{TYPES}u64 Main(Request request) {{ Node caller = request.context.compilation.root; return caller.gen; }}"
    ));
    assert_eq!(three.len(), 3, "context->compilation, compilation->root, caller->gen: {three:?}");
    assert_eq!(four.len(), 4, "request->context plus the three-segment chain: {four:?}");
    assert_eq!(&four[1..], &three[..], "the deeper chain must reuse the exact field offsets of the shorter one");
}

/// `Query.Span(Query.At(request.context.compilation.entryRoot))`: the same four-segment chain
/// spelled directly as a call argument must load the same fields as the `let` form.
#[test]
fn four_segment_projection_as_a_call_argument_matches_the_let_form() {
    let let_clif = main_clif_of_program(&format!(
        "{TYPES}u64 Gen(Node node) {{ return node.gen; }} u64 Main(Request request) {{ Node caller = request.context.compilation.root; return Gen(caller); }}"
    ));
    let argument_clif = main_clif_of_program(&format!(
        "{TYPES}u64 Gen(Node node) {{ return node.gen; }} u64 Main(Request request) {{ return Gen(request.context.compilation.root); }}"
    ));
    let let_form = offsets_in(&let_clif);
    let argument_form = offsets_in(&argument_clif);
    assert_eq!(let_form.len(), 3, "{let_form:?}\n{let_clif}");
    assert_eq!(argument_form, let_form, "argument and let spellings of one chain must load identically\n{argument_clif}\n{let_clif}");
}
