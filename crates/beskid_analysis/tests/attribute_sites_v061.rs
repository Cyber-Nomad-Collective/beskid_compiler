//! 0.6.1: attributes on every declaration site parse into the `attributes` field of their node,
//! and the formatter round-trips each one in the position the grammar gives it.

use beskid_analysis::format::format_program;
use beskid_analysis::services::parse_program;
use beskid_analysis::syntax::{
    ContractNode, Expression, Node, Program, Spanned, Statement,
};

/// One attribute at every site of the attribute target catalogue.
const CORPUS: &str = r#"[Area]
pub mod Inner {
    pub unit Nothing() { }
}

[Meta]
pub attribute Trace(Parameter) { }

[Pinned]
const Limit = 10;

[Shape]
pub type Counter<[Any] T> {
    [Slot]
    i64 count,

    [Reader]
    pub i64 Read([Receiver] this) {
        return this.count;
    }

    pub U Apply<[Any] U>(mut this, (T) => U f, T value) where U: Shown {
        return f(value);
    }
}

[Kind]
pub enum Mode {
    Fast,
    [Legacy]
    Slow,
}

[Api]
pub contract Shown {
    [Assoc]
    type Item;

    [Embed]
    Other;

    [Call]
    string Show();
}

[Block]
impl Counter<i64> {
    pub i64 Twice(this) {
        return this.count * 2;
    }
}

[Entry]
i64 Add([Trace] i64 left, i64 right) {
    [Pinned]
    mut i64 total = 0;
    [Pinned]
    let square = [Pure] ([Hot] i64 x) => x * x;
    let single = [Pure] ([Hot] y) => y;
    return total + left + right;
}

[Check]
test Adds {
    i64 value = 1;
}
"#;

fn parse(source: &str) -> Spanned<Program> {
    parse_program(source).unwrap_or_else(|error| panic!("corpus must parse: {error:?}"))
}

fn names(attributes: &[Spanned<beskid_analysis::syntax::Attribute>]) -> Vec<String> {
    attributes.iter().map(|attribute| attribute.node.name.node.name.clone()).collect()
}

#[test]
fn every_site_carries_its_attributes() {
    let program = parse(CORPUS);
    let items: Vec<&Node> = program.node.items.iter().map(|item| &item.node).collect();

    let Node::InlineModule(module) = items[0] else { panic!("module") };
    assert_eq!(names(&module.node.attributes), ["Area"]);
    let Node::AttributeDeclaration(declaration) = items[1] else { panic!("attribute declaration") };
    assert_eq!(names(&declaration.node.attributes), ["Meta"]);
    let Node::ConstantDefinition(constant) = items[2] else { panic!("constant") };
    assert_eq!(names(&constant.node.attributes), ["Pinned"]);

    let Node::TypeDefinition(counter) = items[3] else { panic!("type") };
    assert_eq!(names(&counter.node.attributes), ["Shape"]);
    assert_eq!(names(&counter.node.generics[0].node.attributes), ["Any"]);
    assert_eq!(counter.node.generics[0].node.name, "T");
    assert_eq!(names(&counter.node.fields[0].node.attributes), ["Slot"]);
    let read = &counter.node.methods[0].node;
    assert_eq!(names(&read.attributes), ["Reader"]);
    let receiver = read.receiver.as_ref().expect("explicit receiver");
    assert_eq!(names(&receiver.node.attributes), ["Receiver"]);
    assert!(!receiver.node.mutable);
    assert!(read.parameters.is_empty());
    let apply = &counter.node.methods[1].node;
    assert_eq!(apply.generics.len(), 1);
    assert_eq!(names(&apply.generics[0].node.attributes), ["Any"]);
    assert!(apply.receiver.as_ref().expect("mut receiver").node.mutable);
    assert_eq!(apply.parameters.len(), 2);
    assert_eq!(apply.where_bounds.len(), 1);

    let Node::EnumDefinition(mode) = items[4] else { panic!("enum") };
    assert_eq!(names(&mode.node.attributes), ["Kind"]);
    assert!(mode.node.variants[0].node.attributes.is_empty());
    assert_eq!(names(&mode.node.variants[1].node.attributes), ["Legacy"]);

    let Node::ContractDefinition(shown) = items[5] else { panic!("contract") };
    assert_eq!(names(&shown.node.attributes), ["Api"]);
    let member_attributes: Vec<Vec<String>> = shown
        .node
        .items
        .iter()
        .map(|item| match &item.node {
            ContractNode::AssociatedType(member) => names(&member.node.attributes),
            ContractNode::Embedding(member) => names(&member.node.attributes),
            ContractNode::MethodSignature(member) => names(&member.node.attributes),
        })
        .collect();
    assert_eq!(member_attributes, [["Assoc"], ["Embed"], ["Call"]]);

    let Node::ImplBlock(block) = items[6] else { panic!("impl block") };
    assert_eq!(names(&block.node.attributes), ["Block"]);
    assert!(block.node.methods[0].node.receiver.is_some());

    let Node::Function(add) = items[7] else { panic!("function") };
    assert_eq!(names(&add.node.attributes), ["Entry"]);
    assert_eq!(names(&add.node.parameters[0].node.attributes), ["Trace"]);
    assert!(add.node.parameters[1].node.attributes.is_empty());
    let statements = &add.node.body.node.statements;
    let Statement::Let(total) = &statements[0].node else { panic!("typed let") };
    assert_eq!(names(&total.node.attributes), ["Pinned"]);
    assert!(total.node.mutable);
    let Statement::Let(square) = &statements[1].node else { panic!("inferred let") };
    assert_eq!(names(&square.node.attributes), ["Pinned"]);
    let Expression::Lambda(lambda) = &square.node.value.node else { panic!("lambda") };
    assert_eq!(names(&lambda.node.attributes), ["Pure"]);
    assert_eq!(names(&lambda.node.parameters[0].node.attributes), ["Hot"]);
    let Statement::Let(single) = &statements[2].node else { panic!("untyped lambda let") };
    let Expression::Lambda(lambda) = &single.node.value.node else { panic!("lambda") };
    assert_eq!(names(&lambda.node.parameters[0].node.attributes), ["Hot"]);
    assert!(lambda.node.parameters[0].node.ty.is_none());

    let Node::TestDefinition(adds) = items[8] else { panic!("test") };
    assert_eq!(names(&adds.node.attributes), ["Check"]);
}

#[test]
fn formatter_round_trips_every_site() {
    let program = parse(CORPUS);
    let once = format_program(&program).expect("format");
    let twice = format_program(&parse(&once)).expect("format again");
    assert_eq!(once, twice, "format must be a fixed point:\n{once}");
    for expected in [
        "[Area]\npub mod Inner",
        "[Meta]\npub attribute Trace(Parameter)",
        "[Pinned]\nconst Limit = 10;",
        "[Shape]\npub type Counter<[Any] T>",
        "[Slot]\n",
        "[Reader]\n",
        "Read([Receiver] this)",
        "Apply<[Any] U>(mut this, U(T) f, T value) where U: Shown",
        "[Legacy]\n",
        "[Assoc]\n",
        "[Embed]\n",
        "[Call]\n",
        "[Block]\nimpl Counter<i64>",
        "Twice(this)",
        "[Entry]\ni64 Add([Trace] i64 left, i64 right)",
        "[Pinned]\n",
        "mut i64 total = 0;",
        "let square = [Pure] ([Hot] i64 x) => x * x;",
        " = [Pure] ([Hot] y) => y;",
        "[Check]\ntest adds",
    ] {
        assert!(once.contains(expected), "formatted output lacks {expected:?}:\n{once}");
    }
}

#[test]
fn spec_scenarios_parse() {
    // language-meta--metaprogramming--metaprogramming, "Attributes on every declaration site".
    let program = parse(
        "pub attribute Receiver(ReceiverParameter) { }\n\
         pub type Counter { i64 count, pub i64 Read([Receiver] this) { return this.count; } }\n\
         unit Keep<[Any] T>(T value) { }",
    );
    let Node::TypeDefinition(counter) = &program.node.items[1].node else { panic!("type") };
    let receiver = counter.node.methods[0].node.receiver.as_ref().expect("receiver");
    assert_eq!(names(&receiver.node.attributes), ["Receiver"]);
    let Node::Function(keep) = &program.node.items[2].node else { panic!("function") };
    assert_eq!(names(&keep.node.generics[0].node.attributes), ["Any"]);
}

#[test]
fn member_access_type_arguments_parse_and_format() {
    let source = "Box<string> Twice(Box<i64> b) { return b.Map<i64>((i64 x) => x * 2).Map<string>((i64 y) => \"${y}\"); }";
    let program = parse(source);
    let Node::Function(twice) = &program.node.items[0].node else { panic!("function") };
    let Statement::Return(ret) = &twice.node.body.node.statements[0].node else { panic!("return") };
    let Some(Expression::Call(outer)) = ret.node.value.as_ref().map(|value| &value.node) else { panic!("call") };
    let Expression::Member(outer_member) = &outer.node.callee.node else { panic!("member callee") };
    assert_eq!(outer_member.node.type_args.len(), 1);
    // `b.Map<i64>` on a local receiver is a path whose last segment carries the arguments.
    let Expression::Call(inner) = &outer_member.node.target.node else { panic!("inner call") };
    let Expression::Path(inner_path) = &inner.node.callee.node else { panic!("inner path callee") };
    assert_eq!(inner_path.node.path.node.segments.last().expect("segment").node.type_args.len(), 1);
    let formatted = format_program(&program).expect("format");
    assert!(formatted.contains(".Map<i64>((i64 x) => x * 2).Map<string>("), "{formatted}");

    // Comparisons after a member access are not type arguments.
    let program = parse("bool Small(Box<i64> b) { return b.item < 10 && b.item > 2; }");
    let Node::Function(small) = &program.node.items[0].node else { panic!("function") };
    let Statement::Return(ret) = &small.node.body.node.statements[0].node else { panic!("return") };
    let text = format!("{:?}", ret.node.value);
    assert!(!text.contains("type_args: [Spanned"), "comparison operands must not carry type arguments: {text}");
}

#[test]
fn receiver_named_like_this_prefix_is_a_parameter() {
    let program = parse("pub type Holder { i64 count, pub i64 Set(thisValue x) { return 0; } }\npub type thisValue { }");
    let Node::TypeDefinition(holder) = &program.node.items[0].node else { panic!("type") };
    let method = &holder.node.methods[0].node;
    assert!(method.receiver.is_none());
    assert_eq!(method.parameters.len(), 1);
}

#[test]
fn corelib_declares_every_recognized_attribute_with_the_enforced_targets() {
    use beskid_analysis::attribute_targets::{COMPILER_RECOGNIZED_ATTRIBUTES, is_attribute_target};
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corelib/packages/foundation/src/Core/Attributes.bd");
    let source = std::fs::read_to_string(path).expect("Core.Attributes source");
    let program = parse(&source);
    let declared: Vec<(String, Vec<String>)> = program
        .node
        .items
        .iter()
        .filter_map(|item| match &item.node {
            Node::AttributeDeclaration(declaration) => Some((
                declaration.node.name.node.name.clone(),
                declaration.node.targets.iter().map(|target| target.node.name.node.name.clone()).collect(),
            )),
            _ => None,
        })
        .collect();
    assert_eq!(declared.len(), COMPILER_RECOGNIZED_ATTRIBUTES.len());
    for attribute in COMPILER_RECOGNIZED_ATTRIBUTES {
        let (_, targets) = declared
            .iter()
            .find(|(name, _)| name == attribute.name)
            .unwrap_or_else(|| panic!("Core.Attributes lacks {}", attribute.name));
        assert_eq!(targets, attribute.targets, "{}", attribute.name);
        assert!(targets.iter().all(|target| is_attribute_target(target)));
    }
}
