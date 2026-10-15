//! 0.6.2: the `move` keyword, `MoveExpression`, `move` parameters and `move this` receivers
//! parse into the syntax model, and the formatter round-trips every `move`, `exchange` and
//! `[Captures]` form (`tooling--formatter--verification-and-traceability`, "move forms in the
//! formatter corpus").

use beskid_analysis::format::format_program;
use beskid_analysis::services::parse_program;
use beskid_analysis::syntax::{Expression, Node, Program, Spanned, Statement};

const CORPUS: &str = r#"[Isolated]
pub type Archive {
    List<u8[]> blocks,
    Option<u8[]> pending,
    u8[] buffer,

    pub Archive Add(move this, move u8[] block) {
        this.blocks = this.blocks.Push(move block);
        return this;
    }

    pub Option<u8[]> Take(move this) {
        return move this.pending;
    }

    pub u8[] Swap(mut this, move u8[] next) {
        return exchange(this.buffer, move next);
    }
}

pub unit Run([Captures(Bound: Transferable)] () => i64 body) {
}

unit Main(move u8[] data, Archive archive) {
    u8[] kept = move data;
    Archive next = archive.Add(move kept);
    return;
}

u8[] Make(move u8[] data) {
    return move data;
}
"#;

fn parse(source: &str) -> Spanned<Program> {
    parse_program(source).unwrap_or_else(|error| panic!("corpus must parse: {error:?}"))
}

#[test]
fn move_modifiers_and_expressions_parse() {
    let program = parse(CORPUS);
    let Node::TypeDefinition(archive) = &program.node.items[0].node else { panic!("type") };
    let add = &archive.node.methods[0].node;
    let receiver = add.receiver.as_ref().expect("explicit receiver");
    assert!(receiver.node.movable && !receiver.node.mutable);
    assert!(add.parameters[0].node.movable && !add.parameters[0].node.mutable);
    let swap = &archive.node.methods[2].node;
    let receiver = swap.receiver.as_ref().expect("mut receiver");
    assert!(receiver.node.mutable && !receiver.node.movable);

    let take = &archive.node.methods[1].node;
    let Statement::Return(ret) = &take.body.node.statements[0].node else { panic!("return") };
    let Some(Expression::Move(moved)) = ret.node.value.as_ref().map(|value| &value.node) else {
        panic!("move expression: {:?}", ret.node.value)
    };
    assert!(matches!(moved.node.operand.node, Expression::Path(_) | Expression::Member(_)));

    let Node::Function(main) = &program.node.items[2].node else { panic!("function") };
    assert!(main.node.parameters[0].node.movable);
    assert!(!main.node.parameters[1].node.movable);
    let Statement::Let(kept) = &main.node.body.node.statements[0].node else { panic!("let") };
    let Expression::Move(moved) = &kept.node.value.node else { panic!("move initializer") };
    assert!(matches!(moved.node.operand.node, Expression::Path(_)));
}

#[test]
fn move_is_reserved_and_does_not_combine() {
    for rejected in [
        "unit F() { i64 move = 1; }",
        "unit F(mut move u8[] block) { }",
        "unit F(move mut u8[] block) { }",
        "unit F(u8[] data) { i64 n = -move data; }",
        "pub type T { i64 x, pub unit G(mut move this) { } }",
    ] {
        assert!(parse_program(rejected).is_err(), "must be rejected: {rejected}");
    }
    // `exchange` stays an ordinary identifier.
    parse("unit F() { i64 exchange = 1; }");
}

#[test]
fn formatter_round_trips_every_move_form() {
    let program = parse(CORPUS);
    let once = format_program(&program).expect("format");
    let twice = format_program(&parse(&once)).expect("format again");
    assert_eq!(once, twice, "format must be a fixed point:\n{once}");
    for expected in [
        "pub Archive Add(move this, move u8[] block) {",
        "this.blocks = this.blocks.Push(move block);",
        "return move this.pending;",
        "pub u8[] Swap(mut this, move u8[] next) {",
        "return exchange(this.buffer, move next);",
        "[Captures(Bound: Transferable)] () => i64 body",
        "u8[] kept = move data;",
        "archive.Add(move kept)",
        "u8[] Make(move u8[] data) {",
        "return move data;",
    ] {
        assert!(once.contains(expected), "missing `{expected}` in:\n{once}");
    }
}
