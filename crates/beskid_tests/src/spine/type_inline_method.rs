//! Inline methods inside `type { }` bodies: parse, dispatch, and duplicate-member rules.

use beskid_analysis::services::parse_program_with_source_name;
use beskid_analysis::syntax::Node;

#[test]
fn type_body_parses_inline_method() {
    let program = parse_program_with_source_name(
        "test.bd",
        r#"
type Counter {
    i32 value,

    pub unit Increment() {
        value += 1;
    }
}
"#,
    )
    .expect("parse type with inline method");
    let Node::TypeDefinition(type_def) = &program.node.items[0].node else {
        panic!("expected type definition");
    };
    assert_eq!(type_def.node.fields.len(), 1);
    assert_eq!(type_def.node.methods.len(), 1);
    assert_eq!(type_def.node.methods[0].node.name.node.name, "Increment");
}

#[test]
fn generic_type_body_parses_inline_method() {
    let program = parse_program_with_source_name(
        "test.bd",
        r#"
type Container<T> {
    T item,

    pub T Get() {
        return item;
    }
}
"#,
    )
    .expect("parse generic type with inline method");
    let Node::TypeDefinition(type_def) = &program.node.items[0].node else {
        panic!("expected type definition");
    };
    assert_eq!(type_def.node.generics.len(), 1);
    assert_eq!(type_def.node.methods.len(), 1);
    assert_eq!(type_def.node.methods[0].node.name.node.name, "Get");
}

#[test]
fn duplicate_field_and_method_name_is_parseable() {
    // Parser accepts the surface; resolution reports duplicate member.
    let program = parse_program_with_source_name(
        "test.bd",
        r#"
type Bad {
    i32 value,

    pub unit value() {
        return;
    }
}
"#,
    )
    .expect("parse type with duplicate field/method name");
    let Node::TypeDefinition(type_def) = &program.node.items[0].node else {
        panic!("expected type definition");
    };
    assert_eq!(type_def.node.fields.len(), 1);
    assert_eq!(type_def.node.methods.len(), 1);
}
