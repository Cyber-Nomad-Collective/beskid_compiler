use std::fs;
use std::path::PathBuf;

use beskid_isle::{
    NodeKind, SyntaxNodeClassification, UNSUPPORTED_TYPED_OPERATION_KINDS, classify_syntax_node_kind,
    syntax_node_kind_catalogue, unsupported_typed_operation_kinds,
};
use beskid_queries::IndexedNodeKind;

#[test]
fn expanded_syntax_catalogue_is_total_unique_deterministic_and_surjective() {
    let catalogue = syntax_node_kind_catalogue().collect::<Vec<_>>();
    let repeated = syntax_node_kind_catalogue().collect::<Vec<_>>();

    assert_eq!(catalogue, repeated, "catalogue order must be deterministic");
    assert_eq!(catalogue.len(), IndexedNodeKind::ALL.len());
    assert_eq!(
        catalogue.iter().map(|(syntax, _)| *syntax).collect::<std::collections::HashSet<_>>().len(),
        IndexedNodeKind::ALL.len(),
        "every authoritative syntax kind must occur exactly once"
    );
    assert!(catalogue.iter().zip(IndexedNodeKind::ALL).all(|((actual, classification), expected)| {
        actual == expected && *classification == classify_syntax_node_kind(*expected)
    }));

    for isle_kind in NodeKind::ALL {
        assert!(
            catalogue
                .iter()
                .any(|(_, classification)| { *classification == SyntaxNodeClassification::IsleLowered(*isle_kind) }),
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
        assert!(!from_roster[..index].contains(kind), "unsupported roster must not contain duplicates: {kind:?}");
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

/// CYB-81: every remaining UnsupportedTypedOperation kind is intentionally rejected for
/// 0.4 W4.1 (not a pending inventory port). Later waves own composition / closures.
#[test]
fn unsupported_kinds_are_intentionally_release_rejected_for_0_4() {
    use IndexedNodeKind as Syntax;

    let dispositions: &[(IndexedNodeKind, &str)] = &[
        (Syntax::HostDefinition, "composition declaration; not an executable ISLE item"),
        (Syntax::RegistryBlock, "composition declaration; not an executable ISLE item"),
        (Syntax::RegistryEntry, "composition declaration; not an executable ISLE item"),
        (Syntax::ScopeDefinition, "composition declaration; not an executable ISLE item"),
        (Syntax::ScopeHook, "composition declaration; not an executable ISLE item"),
        (Syntax::WithStatement, "composition scope bracket waits on container facts (W5/composition)"),
        (Syntax::LaunchStatement, "composition launch bracket waits on container facts (W5/composition)"),
        (Syntax::CodeStringLiteral, "fenced code strings unsupported in both HIR and ISLE paths"),
    ];

    assert_eq!(
        dispositions.len(),
        UNSUPPORTED_TYPED_OPERATION_KINDS.len(),
        "every unsupported kind must carry an explicit 0.4 rejection rationale"
    );
    for (kind, rationale) in dispositions {
        assert_eq!(
            classify_syntax_node_kind(*kind),
            SyntaxNodeClassification::UnsupportedTypedOperation,
            "{kind:?}: {rationale}"
        );
        assert!(
            UNSUPPORTED_TYPED_OPERATION_KINDS.contains(kind),
            "{kind:?} must remain on the unsupported roster: {rationale}"
        );
        assert!(!rationale.is_empty(), "{kind:?} must document why it stays rejected");
    }
    assert_eq!(
        classify_syntax_node_kind(IndexedNodeKind::SpawnExpression),
        SyntaxNodeClassification::IsleLowered(NodeKind::SpawnExpression),
        "spawn is production-supported at the inventory boundary (zero-arg entry facts)"
    );
    assert_eq!(
        classify_syntax_node_kind(IndexedNodeKind::MethodDefinition),
        SyntaxNodeClassification::IsleLowered(NodeKind::MethodDefinition),
        "methods are production-supported at the inventory boundary"
    );
}

/// Every IsleLowered node kind must name an on-disk verified CLIF (or item-emission) regression.
#[test]
fn every_isle_lowered_kind_has_verified_clif_evidence() {
    let isle_tests = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    let codegen_tests = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("beskid_codegen").join("tests");
    let evidence: &[(NodeKind, PathBuf)] = &[
        (NodeKind::Program, isle_tests.join("rule_coverage.rs")),
        (NodeKind::FunctionDefinition, isle_tests.join("function_emitter.rs")),
        (NodeKind::TestDefinition, codegen_tests.join("isle_adapter.rs")),
        (NodeKind::MethodDefinition, codegen_tests.join("isle_adapter.rs")),
        (NodeKind::ExpressionStatement, isle_tests.join("statement_emitter.rs")),
        (NodeKind::ReturnStatement, isle_tests.join("statement_emitter.rs")),
        (NodeKind::LetStatement, isle_tests.join("locals.rs")),
        (NodeKind::IfStatement, isle_tests.join("if_else.rs")),
        (NodeKind::WhileStatement, isle_tests.join("while_transfer.rs")),
        (NodeKind::BreakStatement, isle_tests.join("while_transfer.rs")),
        (NodeKind::ContinueStatement, isle_tests.join("while_transfer.rs")),
        (NodeKind::LiteralExpression, isle_tests.join("leaf_clif.rs")),
        (NodeKind::GroupedExpression, isle_tests.join("leaf_clif.rs")),
        (NodeKind::UnaryExpression, isle_tests.join("leaf_clif.rs")),
        (NodeKind::BinaryExpression, isle_tests.join("leaf_clif.rs")),
        (NodeKind::AssignExpression, isle_tests.join("locals.rs")),
        (NodeKind::CallExpression, isle_tests.join("direct_calls.rs")),
        (NodeKind::PathExpression, isle_tests.join("locals.rs")),
        (NodeKind::IndexExpression, isle_tests.join("array_memory.rs")),
        (NodeKind::ArrayLiteralExpression, isle_tests.join("array_memory.rs")),
        (NodeKind::FieldExpression, isle_tests.join("struct_memory.rs")),
        (NodeKind::StructLiteralExpression, isle_tests.join("struct_memory.rs")),
        (NodeKind::EnumLiteralExpression, isle_tests.join("enum_match.rs")),
        (NodeKind::MatchExpression, isle_tests.join("enum_match.rs")),
        (NodeKind::RangeExpression, isle_tests.join("block_range_for.rs")),
        (NodeKind::BlockExpression, isle_tests.join("block_sequence.rs")),
        (NodeKind::ForStatement, isle_tests.join("block_range_for.rs")),
        (NodeKind::SpawnExpression, codegen_tests.join("parsed_project_isle_harness.rs")),
        (NodeKind::LambdaExpression, codegen_tests.join("parsed_project_isle_harness.rs")),
        (NodeKind::ClifBlock, isle_tests.join("clif_block.rs")),
    ];

    assert_eq!(evidence.len(), NodeKind::ALL.len(), "CLIF evidence table must cover every NodeKind exactly once");
    for kind in NodeKind::ALL {
        assert!(evidence.iter().any(|(covered, _)| covered == kind), "missing CLIF evidence row for {kind:?}");
    }
    for (kind, path) in evidence {
        assert!(path.is_file(), "missing CLIF evidence for {kind:?}: {}", path.display());
    }
}

/// Rejection evidence status for each unsupported kind.
///
/// `Present` names an existing span-bearing diagnostic regression in `beskid_codegen` tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UnsupportedEvidence {
    Present(&'static str),
}

#[test]
fn every_unsupported_kind_has_rejection_evidence_or_codex_blocker() {
    use IndexedNodeKind as Syntax;
    use UnsupportedEvidence::Present;

    let evidence: &[(IndexedNodeKind, UnsupportedEvidence)] = &[
        (Syntax::HostDefinition, Present("isle_adapter.rs")),
        (Syntax::RegistryBlock, Present("isle_adapter.rs")),
        (Syntax::RegistryEntry, Present("isle_adapter.rs")),
        (Syntax::ScopeDefinition, Present("isle_adapter.rs")),
        (Syntax::ScopeHook, Present("isle_adapter.rs")),
        (Syntax::WithStatement, Present("isle_adapter.rs")),
        (Syntax::LaunchStatement, Present("isle_adapter.rs")),
        (Syntax::CodeStringLiteral, Present("isle_adapter.rs")),
    ];

    assert_eq!(
        evidence.len(),
        UNSUPPORTED_TYPED_OPERATION_KINDS.len(),
        "rejection evidence table must cover exactly the unsupported roster"
    );
    for kind in UNSUPPORTED_TYPED_OPERATION_KINDS {
        assert!(evidence.iter().any(|(covered, _)| covered == kind), "missing rejection evidence row for {kind:?}");
    }

    let codegen_tests = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("beskid_codegen").join("tests");

    for (kind, status) in evidence {
        assert_eq!(classify_syntax_node_kind(*kind), SyntaxNodeClassification::UnsupportedTypedOperation, "{kind:?}");
        let UnsupportedEvidence::Present(relative) = status;
        let path = codegen_tests.join(relative);
        assert!(path.is_file(), "missing span-bearing rejection fixture for {kind:?}: {}", path.display());
    }
}

#[test]
fn typed_operation_families_have_explicit_classifications() {
    use IndexedNodeKind as Syntax;
    use SyntaxNodeClassification::{IsleLowered, Structural};

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
        (Syntax::ArrayLiteralExpression, NodeKind::ArrayLiteralExpression),
        (Syntax::MemberExpression, NodeKind::FieldExpression),
        (Syntax::StructLiteralExpression, NodeKind::StructLiteralExpression),
        (Syntax::EnumConstructorExpression, NodeKind::EnumLiteralExpression),
    ] {
        assert_eq!(classify_syntax_node_kind(syntax), IsleLowered(isle));
    }

    assert_eq!(classify_syntax_node_kind(Syntax::LambdaExpression), IsleLowered(NodeKind::LambdaExpression));
    assert_eq!(classify_syntax_node_kind(Syntax::SpawnExpression), IsleLowered(NodeKind::SpawnExpression));
    assert_eq!(classify_syntax_node_kind(Syntax::Identifier), Structural);
}

/// The canonical corelib manifests depend on local declarations and mutable writes.
/// Keep their source syntax on the generated ISLE route: `CodegenInput` supplies facts,
/// and these rules select the existing constructor implementations without a HIR fallback.
#[test]
fn generated_isle_routes_local_declarations_and_assignments() {
    let isle = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("isle");
    let statements = fs::read_to_string(isle.join("statements.isle")).expect("read statement ISLE rules");
    let memory = fs::read_to_string(isle.join("memory.isle")).expect("read memory ISLE rules");

    assert!(
        statements
            .contains("(rule (lower_statement key @ (node_kind (NodeKind.LetStatement)))\n      (emit_local_let key))"),
        "LetStatement must dispatch through generated ISLE"
    );

    for (target, constructor) in [
        ("PathExpression", "emit_local_assign"),
        ("FieldExpression", "emit_field_assign"),
        ("IndexExpression", "emit_index_assign"),
    ] {
        assert!(
            memory.contains(&format!(
                "(node_kind (NodeKind.AssignExpression))\n        (assignment_target_kind (NodeKind.{target}))))\n      ({constructor} key)"
            )),
            "AssignExpression targeting {target} must dispatch through generated ISLE"
        );
    }
}

#[test]
fn binary_and_unary_operator_facts_have_isle_rules() {
    let isle = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("isle");
    let source = ["binary.isle", "unary_casts.isle", "control_flow.isle", "dispatch.isle"]
        .into_iter()
        .map(|name| fs::read_to_string(isle.join(name)).expect("read ISLE rules"))
        .collect::<String>();

    for operator in [
        "Or",
        "And",
        "BitOr",
        "BitAnd",
        "Shl",
        "Shr",
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
        "EnumEq",
        "EnumNotEq",
    ] {
        assert!(source.contains(&format!("OperatorFact.{operator}")), "missing ISLE rule for {operator}");
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
        assert!(source.contains("(rule"), "{group} contains no real ISLE rule");
    }
}
