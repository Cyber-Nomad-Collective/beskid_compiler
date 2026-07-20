use std::fs;
use std::path::PathBuf;

use beskid_isle::{
    classify_syntax_node_kind, syntax_node_kind_catalogue, unsupported_typed_operation_kinds,
    NodeKind, SyntaxNodeClassification, UNSUPPORTED_TYPED_OPERATION_KINDS,
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
fn unsupported_roster_is_bijective_with_classify() {
    let from_classify = unsupported_typed_operation_kinds().collect::<Vec<_>>();
    let from_roster = UNSUPPORTED_TYPED_OPERATION_KINDS.to_vec();

    assert_eq!(
        from_classify, from_roster,
        "UNSUPPORTED_TYPED_OPERATION_KINDS must equal every UnsupportedTypedOperation catalogue entry"
    );
    for (index, kind) in from_roster.iter().enumerate() {
        assert!(
            !from_roster[..index].contains(kind),
            "unsupported roster must not contain duplicates: {kind:?}"
        );
        assert_eq!(
            classify_syntax_node_kind(*kind),
            SyntaxNodeClassification::UnsupportedTypedOperation,
            "{kind:?} must classify as unsupported"
        );
    }
}

#[test]
fn method_definitions_are_production_supported_isle_items() {
    assert_eq!(
        classify_syntax_node_kind(IndexedNodeKind::MethodDefinition),
        SyntaxNodeClassification::IsleLowered(NodeKind::MethodDefinition),
        "methods are executable ISLE items, not unsupported typed operations"
    );
    assert!(
        !UNSUPPORTED_TYPED_OPERATION_KINDS.contains(&IndexedNodeKind::MethodDefinition),
        "MethodDefinition must leave the unsupported roster once production-supported"
    );
}

/// Every IsleLowered node kind must name an on-disk verified CLIF (or item-emission) regression.
#[test]
fn every_isle_lowered_kind_has_verified_clif_evidence() {
    let isle_tests = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    let codegen_tests = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("beskid_codegen")
        .join("tests");
    let evidence: &[(NodeKind, PathBuf)] = &[
        (NodeKind::Program, isle_tests.join("rule_coverage.rs")),
        (
            NodeKind::FunctionDefinition,
            isle_tests.join("function_emitter.rs"),
        ),
        (
            NodeKind::TestDefinition,
            codegen_tests.join("isle_adapter.rs"),
        ),
        (
            NodeKind::MethodDefinition,
            codegen_tests.join("isle_adapter.rs"),
        ),
        (
            NodeKind::ExpressionStatement,
            isle_tests.join("statement_emitter.rs"),
        ),
        (
            NodeKind::ReturnStatement,
            isle_tests.join("statement_emitter.rs"),
        ),
        (NodeKind::LetStatement, isle_tests.join("locals.rs")),
        (NodeKind::IfStatement, isle_tests.join("if_else.rs")),
        (
            NodeKind::WhileStatement,
            isle_tests.join("while_transfer.rs"),
        ),
        (
            NodeKind::BreakStatement,
            isle_tests.join("while_transfer.rs"),
        ),
        (
            NodeKind::ContinueStatement,
            isle_tests.join("while_transfer.rs"),
        ),
        (NodeKind::LiteralExpression, isle_tests.join("leaf_clif.rs")),
        (NodeKind::GroupedExpression, isle_tests.join("leaf_clif.rs")),
        (NodeKind::UnaryExpression, isle_tests.join("leaf_clif.rs")),
        (NodeKind::BinaryExpression, isle_tests.join("leaf_clif.rs")),
        (NodeKind::AssignExpression, isle_tests.join("locals.rs")),
        (NodeKind::CallExpression, isle_tests.join("direct_calls.rs")),
        (NodeKind::PathExpression, isle_tests.join("locals.rs")),
        (
            NodeKind::IndexExpression,
            isle_tests.join("array_memory.rs"),
        ),
        (
            NodeKind::ArrayLiteralExpression,
            isle_tests.join("array_memory.rs"),
        ),
        (
            NodeKind::FieldExpression,
            isle_tests.join("struct_memory.rs"),
        ),
        (
            NodeKind::StructLiteralExpression,
            isle_tests.join("struct_memory.rs"),
        ),
        (
            NodeKind::EnumLiteralExpression,
            isle_tests.join("enum_match.rs"),
        ),
        (NodeKind::MatchExpression, isle_tests.join("enum_match.rs")),
        (
            NodeKind::RangeExpression,
            isle_tests.join("block_range_for.rs"),
        ),
        (
            NodeKind::BlockExpression,
            isle_tests.join("block_sequence.rs"),
        ),
        (
            NodeKind::ForStatement,
            isle_tests.join("block_range_for.rs"),
        ),
        (
            NodeKind::SpawnExpression,
            codegen_tests.join("parsed_project_isle_harness.rs"),
        ),
    ];

    assert_eq!(
        evidence.len(),
        NodeKind::ALL.len(),
        "CLIF evidence table must cover every NodeKind exactly once"
    );
    for kind in NodeKind::ALL {
        assert!(
            evidence.iter().any(|(covered, _)| covered == kind),
            "missing CLIF evidence row for {kind:?}"
        );
    }
    for (kind, path) in evidence {
        assert!(
            path.is_file(),
            "missing CLIF evidence for {kind:?}: {}",
            path.display()
        );
    }
}

/// Rejection evidence status for each unsupported kind.
///
/// `Present` names an existing span-bearing diagnostic regression. `CodexBlocker` records the
/// remaining classification gap for CYB-81 (host composition, try, code strings).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UnsupportedEvidence {
    Present(&'static str),
    CodexBlocker(&'static str),
}

#[test]
fn every_unsupported_kind_has_rejection_evidence_or_codex_blocker() {
    use IndexedNodeKind as Syntax;
    use UnsupportedEvidence::{CodexBlocker, Present};

    let evidence: &[(IndexedNodeKind, UnsupportedEvidence)] = &[
        (
            Syntax::HostDefinition,
            CodexBlocker("CYB-81 host-composition: HostDefinition span-bearing rejection fixture"),
        ),
        (
            Syntax::RegistryBlock,
            CodexBlocker("CYB-81 host-composition: RegistryBlock span-bearing rejection fixture"),
        ),
        (
            Syntax::RegistryEntry,
            CodexBlocker("CYB-81 host-composition: RegistryEntry span-bearing rejection fixture"),
        ),
        (
            Syntax::ScopeDefinition,
            CodexBlocker("CYB-81 host-composition: ScopeDefinition span-bearing rejection fixture"),
        ),
        (
            Syntax::ScopeHook,
            CodexBlocker("CYB-81 host-composition: ScopeHook span-bearing rejection fixture"),
        ),
        (
            Syntax::WithStatement,
            CodexBlocker("CYB-81 host-composition: WithStatement span-bearing rejection fixture"),
        ),
        (
            Syntax::LaunchStatement,
            CodexBlocker("CYB-81 host-composition: LaunchStatement span-bearing rejection fixture"),
        ),
        (Syntax::CodeStringLiteral, Present("isle_adapter.rs")),
        (
            Syntax::TryExpression,
            CodexBlocker("CYB-81 try: TryExpression span-bearing rejection fixture"),
        ),
        (Syntax::LambdaExpression, Present("isle_adapter.rs")),
    ];

    assert_eq!(
        evidence.len(),
        UNSUPPORTED_TYPED_OPERATION_KINDS.len(),
        "rejection evidence table must cover exactly the unsupported roster"
    );
    for kind in UNSUPPORTED_TYPED_OPERATION_KINDS {
        assert!(
            evidence.iter().any(|(covered, _)| covered == kind),
            "missing rejection evidence row for {kind:?}"
        );
    }

    let codegen_tests = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("beskid_codegen")
        .join("tests");

    for (kind, status) in evidence {
        assert_eq!(
            classify_syntax_node_kind(*kind),
            SyntaxNodeClassification::UnsupportedTypedOperation,
            "{kind:?}"
        );
        match status {
            Present(relative) => {
                let path = codegen_tests.join(relative);
                assert!(
                    path.is_file(),
                    "missing span-bearing rejection fixture for {kind:?}: {}",
                    path.display()
                );
            }
            CodexBlocker(reason) => {
                assert!(
                    reason.contains("CYB-81"),
                    "Codex blocker for {kind:?} must cite CYB-81: {reason}"
                );
            }
        }
    }
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
        (Syntax::MethodDefinition, NodeKind::MethodDefinition),
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
        IsleLowered(NodeKind::SpawnExpression)
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
