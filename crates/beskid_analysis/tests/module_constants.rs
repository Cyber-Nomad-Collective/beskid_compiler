use beskid_analysis::services::parse_program_with_source_name;
use beskid_analysis::syntax::{Literal, Node};

#[test]
fn module_integer_constant_is_a_single_source_declaration() {
    let program = parse_program_with_source_name(
        "Runtime/Layout.bd",
        "const SLOT_SIZE = 16;\nword Slots(word count) { return count * SLOT_SIZE; }",
    )
    .expect("module constant source parses");

    let Node::ConstantDefinition(constant) = &program.node.items[0].node else {
        panic!("first item must be a module constant");
    };
    assert_eq!(constant.node.name.node.name, "SLOT_SIZE");
    assert_eq!(constant.node.value.node, Literal::Integer("16".into()));
}

#[test]
fn typed_uninitialized_word_local_is_lowered_to_a_word_zero_initializer() {
    let program =
        parse_program_with_source_name("Runtime/Layout.bd", "word Slots() { word slotIndex; return slotIndex; }")
            .expect("typed uninitialized local parses");
    let Node::Function(function) = &program.node.items[0].node else {
        panic!("first item must be a function");
    };
    let statement = &function.node.body.node.statements[0];
    let beskid_analysis::syntax::Statement::Let(local) = &statement.node else {
        panic!("first body statement must be a local binding");
    };
    let beskid_analysis::syntax::Expression::Literal(literal) = &local.node.value.node else {
        panic!("default initializer must be a literal");
    };
    assert_eq!(literal.node.literal.node, Literal::Integer("0_i64".into()));
}

#[test]
fn hexadecimal_all_ones_word_literal_parses_without_losing_its_spelling() {
    let program = parse_program_with_source_name(
        "Runtime/Layout.bd",
        "word IsFree(word value) { if value == 0xFFFFFFFFFFFFFFFF { return 1; } return 0; }",
    )
    .expect("hexadecimal word literal parses");
    assert_eq!(program.node.items.len(), 1);
}
