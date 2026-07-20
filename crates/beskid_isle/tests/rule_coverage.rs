use std::fs;
use std::path::PathBuf;

use beskid_isle::{
    NodeKind, SyntaxNodeClassification, classify_syntax_node_kind, syntax_node_kind_catalogue,
};
use beskid_queries::IndexedNodeKind;

#[test]
fn expanded_syntax_catalogue_is_total_unique_deterministic_and_surjective() {
    let catalogue = syntax_node_kind_catalogue().collect::<Vec<_>>();
    let repeated = syntax_node_kind_catalogue().collect::<Vec<_>>();

    assert_eq!(catalogue, repeated, "catalogue order must be deterministic");
    assert_eq!(catalogue.len(), IndexedNodeKind::ALL.len());
    assert_eq!(
        catalogue
            .iter()
            .map(|(syntax, _)| *syntax)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        IndexedNodeKind::ALL.len(),
        "every authoritative syntax kind must occur exactly once"
    );
    assert!(catalogue.iter().zip(IndexedNodeKind::ALL).all(
        |((actual, classification), expected)| {
            actual == expected && *classification == classify_syntax_node_kind(*expected)
        }
    ));

    for isle_kind in NodeKind::ALL {
        assert!(
            catalogue.iter().any(|(_, classification)| {
                *classification == SyntaxNodeClassification::IsleLowered(*isle_kind)
            }),
            "orphaned ISLE node kind: {isle_kind:?}"
        );
    }

    assert_eq!(
        classify_syntax_node_kind(IndexedNodeKind::LiteralExpression),
        classify_syntax_node_kind(IndexedNodeKind::Literal),
        "literal wrapper and leaf are an intentional ISLE alias"
    );
    assert_eq!(
        classify_syntax_node_kind(IndexedNodeKind::Block),
        classify_syntax_node_kind(IndexedNodeKind::BlockExpression),
        "statement and expression blocks are an intentional ISLE alias"
    );
}

#[test]
fn typed_operation_families_have_explicit_classifications() {
    use IndexedNodeKind as Syntax;
    use SyntaxNodeClassification::{IsleLowered, Structural, UnsupportedTypedOperation};

    for (syntax, isle) in [
        // Semantic call subfamilies are covered independently in the codegen adapter tests.
        (Syntax::CallExpression, NodeKind::CallExpression),
        // Locals.
        (Syntax::LetStatement, NodeKind::LetStatement),
        (Syntax::PathExpression, NodeKind::PathExpression),
        // Unary operations; contextual cast facts on their source expressions have independent
        // adapter coverage.
        (Syntax::UnaryExpression, NodeKind::UnaryExpression),
        // Control flow.
        (Syntax::IfStatement, NodeKind::IfStatement),
        (Syntax::WhileStatement, NodeKind::WhileStatement),
        (Syntax::ForStatement, NodeKind::ForStatement),
        // Executable items.
        (Syntax::FunctionDefinition, NodeKind::FunctionDefinition),
        (Syntax::TestDefinition, NodeKind::TestDefinition),
        // Aggregates and aggregate memory access.
        (
            Syntax::ArrayLiteralExpression,
            NodeKind::ArrayLiteralExpression,
        ),
        (Syntax::MemberExpression, NodeKind::FieldExpression),
        (
            Syntax::StructLiteralExpression,
            NodeKind::StructLiteralExpression,
        ),
        (
            Syntax::EnumConstructorExpression,
            NodeKind::EnumLiteralExpression,
        ),
    ] {
        assert_eq!(classify_syntax_node_kind(syntax), IsleLowered(isle));
    }

    assert_eq!(
        classify_syntax_node_kind(Syntax::LambdaExpression),
        UnsupportedTypedOperation
    );
    assert_eq!(
        classify_syntax_node_kind(Syntax::SpawnExpression),
        UnsupportedTypedOperation
    );
    assert_eq!(classify_syntax_node_kind(Syntax::Identifier), Structural);
}

#[test]
fn binary_and_unary_operator_facts_have_isle_rules() {
    let isle = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("isle");
    let source = [
        "binary.isle",
        "unary_casts.isle",
        "control_flow.isle",
        "dispatch.isle",
    ]
        .into_iter()
        .map(|name| fs::read_to_string(isle.join(name)).expect("read ISLE rules"))
        .collect::<String>();

    for operator in [
        "Or",
        "And",
        "IdentityEq",
        "IdentityNotEq",
        "Eq",
        "NotEq",
        "Lt",
        "Lte",
        "Gt",
        "Gte",
        "Add",
        "Sub",
        "Mul",
        "Div",
        "Mod",
        "Neg",
        "Not",
        "StringAdd",
        "StringEq",
        "StringNotEq",
    ] {
        assert!(
            source.contains(&format!("OperatorFact.{operator}")),
            "missing ISLE rule for {operator}"
        );
    }
}

#[test]
fn every_owned_rule_group_contains_a_real_rule() {
    let isle = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("isle");
    for group in [
        "expressions.isle",
        "literals.isle",
        "binary.isle",
        "unary_casts.isle",
        "calls.isle",
        "statements.isle",
        "control_flow.isle",
        "memory.isle",
        "runtime_intrinsics.isle",
        "dispatch.isle",
        "items.isle",
    ] {
        let source = fs::read_to_string(isle.join(group)).expect("read ISLE rule group");
        assert!(
            source.contains("(rule"),
            "{group} contains no real ISLE rule"
        );
    }
}
