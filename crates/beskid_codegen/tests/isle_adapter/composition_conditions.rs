use super::support::{
    SyntaxModuleItem, find_function_definitions, item_fixture_with_root, item_name, lower_syntax_program,
};

#[test]
fn compound_integer_argument_retains_its_resolved_word_type_at_a_call_boundary() {
    let (input, isa, root) = item_fixture_with_root(
        "const ENTRY_MAX = 256; word Allocate(word size, word alignment) { return size; } word Main() { return Allocate(ENTRY_MAX * 8, 8); }",
    );
    let items = find_function_definitions(input.database(), root)
        .into_iter()
        .map(|key| SyntaxModuleItem {
            symbol: item_name(input.database(), key).expect("item name query").expect("item name").to_string(),
            key,
        })
        .collect::<Vec<_>>();

    let artifact = lower_syntax_program(&input, isa.as_ref(), &items)
        .expect("a compound word argument retains its resolved type at an exact call boundary");
    let main = artifact.functions.iter().find(|function| function.name == "Main").expect("lowered Main function");
    let clif = main.function.display().to_string();

    assert!(clif.contains("imul"), "the compound size expression must lower:\n{clif}");
    assert!(clif.contains("call"), "the direct call must lower:\n{clif}");
}
