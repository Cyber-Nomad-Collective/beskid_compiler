//! `beskid check` obligations of one item beyond the lowering legality gate and the typing
//! obligations: the legacy `TypeChecker` classes that have no lowering fact of their own.
//!
//! Each obligation is a positive description of one user error at its exact legacy site,
//! derived from generation-bound facts (`node_type`, `spawn_legality`, `event_operation`,
//! `for_iterator_fact`, `call_lowering`, `field_access_receiver`, `call_abi_signature`) and the
//! registered syntax. The fact is fail-closed in the query direction exactly like
//! `typing_obligations`: a position is judged only when the facts prove the shape the legacy
//! checker rejected; unproven shapes (nominal values represented as `pointer`, generic
//! parameters, inferred lambdas) are never findings.
//!
//! The collectors live in `gate/operators.rs` (E1209, E1210, binary E1206, T0905),
//! `gate/control.rs` (E1215-E1218, E1219, E1221, E1223-E1225) and `gate/expressions.rs`
//! (E1102, E1201, E1202, E1204, E1211, E1213, E1228, E1606, E1610).

use super::*;

mod control;
mod expressions;
mod operators;

/// One `check` obligation an item fails. Codes name the legacy `SemanticIssueKind` rendered by
/// [`check_gate_obligations`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum GateObligationKind {
    /// E1209: a binary operator applied to proven primitive operands it does not accept.
    InvalidBinaryOp,
    /// E1210: a unary operator applied to a proven primitive operand it does not accept.
    InvalidUnaryOp,
    /// E1206: binary operands of two different, non-numeric proven primitives.
    OperandMismatch { expected: SemanticTypeId, actual: SemanticTypeId },
    /// T0905: a suffixed integer literal or a float literal outside its primitive's range.
    NumericLiteralOutOfRange { primitive: Arc<str> },
    /// E1223: a `spawn` whose target is not a zero-argument callable entry.
    SpawnTargetNotFiberCompatible,
    /// E1224: a fiber join of a handle owned by an ancestor fiber of the joining fiber.
    JoinWouldDeadlock,
    /// E1225: a `spawn` lambda capturing a stack reference.
    StackReferenceEscapesSpawn,
    /// E1215: a `for` iterable of a proven primitive type, or a nominal type without `Next`.
    NonIterableForTarget,
    /// E1216: the iterable's `Next` method declares parameters.
    IterableNextArityMismatch { expected: usize, actual: usize },
    /// E1217: the iterable's `Next` method does not return `Option<...>`.
    IterableNextReturnNotOption,
    /// E1218: the iterable's `Next` method returns an `Option` applied to other than one type.
    IterableOptionSomeArityMismatch { expected: usize, actual: usize },
    /// E1219: an event raised outside a method of the type that declares the event.
    InvalidEventInvocationScope,
    /// E1221: a lambda subscribed (`+=`/`-=`) to a target that is not an event field.
    InvalidEventSubscriptionTarget,
    /// E1202: an index, array literal, or compound assignment the type system does not support.
    UnsupportedExpression,
    /// E1202: a lambda parameter without a type annotation and without an expected signature.
    MissingTypeAnnotation { name: Arc<str> },
    /// E1228: a primitive conversion applied to a proven non-numeric argument.
    InvalidPrimitiveConversionArgument,
    /// E1213: a member access on a value of proven primitive type.
    InvalidMemberTarget,
    /// E1201: a member reference that names a method as a value.
    UnknownValueType,
    /// E1201: a struct literal whose type path names an enum.
    UnknownStructType,
    /// E1201: an enum constructor whose type path names a struct.
    UnknownEnumType,
    /// E1211: a method call whose receiver type declares neither the method nor a field.
    UnknownStructField { name: Arc<str> },
    /// E1606: a call through a member that is a non-callable field.
    UnknownCallTarget,
    /// E1204: an explicit type-argument list whose count differs from the callee's generics.
    GenericArgumentMismatch { expected: usize, actual: usize },
    /// E1102: a second `let` of the same name in one block, or a repeated parameter name.
    DuplicateLocal { name: Arc<str>, previous: SourceSpan },
    /// E1610: a call whose concrete type does not satisfy a `where` bound of the callee.
    GenericBoundNotSatisfied { type_name: Arc<str>, contract_name: Arc<str> },
}

/// A failed `check` obligation and the exact node that carries the diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct GateObligation {
    pub kind: GateObligationKind,
    pub site: AstNodeKey,
}

/// Report every `check` obligation `key` (a function, method, or test item) fails, in source
/// order. Non-item nodes contain no fact.
pub fn gate_obligations(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<Arc<[GateObligation]>> {
    with_registered_syntax(db, key, gate_obligations_tracked)
}

#[salsa::tracked(persist)]
fn gate_obligations_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<Arc<[GateObligation]>> {
    with_node(db, syntax, key, |program, index, node| {
        if !matches!(
            node.node_kind(),
            NodeKind::FunctionDefinition | NodeKind::MethodDefinition | NodeKind::TestDefinition
        ) {
            return None;
        }
        let mut findings = Vec::new();
        operators::collect_operator_obligations(db, program, index, key, &mut findings);
        operators::collect_literal_obligations(program, index, key, &mut findings);
        control::collect_spawn_obligations(db, program, index, key, &mut findings);
        control::collect_iterator_obligations(db, program, index, key, &mut findings);
        control::collect_event_obligations(db, program, index, key, &mut findings);
        expressions::collect_expression_obligations(db, program, index, key, &mut findings);
        expressions::collect_member_obligations(db, program, index, key, &mut findings);
        expressions::collect_call_obligations(db, program, index, key, &mut findings);
        expressions::collect_duplicate_local_obligations(program, index, key, &mut findings);
        findings.sort_by_key(|finding| finding.site.node.0);
        Some(Arc::from(findings))
    })
}

/// Evaluate the `check` obligations of every item in `items`, collecting every finding of the
/// pass, rendered with the legacy codes. Items whose facts are unavailable are not findings.
pub fn check_gate_obligations(db: &dyn Db, items: &[AstNodeKey]) -> Vec<SemanticFinding> {
    let mut findings = Vec::new();
    for &item in items {
        let Ok(Some(obligations)) = gate_obligations(db, item) else { continue };
        for obligation in obligations.iter() {
            let kind = match &obligation.kind {
                GateObligationKind::InvalidBinaryOp => SemanticIssueKind::TypeInvalidBinaryOp,
                GateObligationKind::InvalidUnaryOp => SemanticIssueKind::TypeInvalidUnaryOp,
                GateObligationKind::OperandMismatch { expected, actual } => SemanticIssueKind::TypeMismatch {
                    expected_name: expected.display_name(),
                    actual_name: actual.display_name(),
                },
                GateObligationKind::NumericLiteralOutOfRange { primitive } => {
                    SemanticIssueKind::NumericLiteralOutOfRange { primitive: primitive.to_string() }
                }
                GateObligationKind::SpawnTargetNotFiberCompatible => SemanticIssueKind::SpawnTargetNotFiberCompatible,
                GateObligationKind::JoinWouldDeadlock => SemanticIssueKind::JoinWouldDeadlock,
                GateObligationKind::StackReferenceEscapesSpawn => SemanticIssueKind::StackReferenceEscapesSpawn,
                GateObligationKind::NonIterableForTarget => SemanticIssueKind::TypeNonIterableForTarget,
                GateObligationKind::IterableNextArityMismatch { expected, actual } => {
                    SemanticIssueKind::TypeIterableNextArityMismatch { expected: *expected, actual: *actual }
                }
                GateObligationKind::IterableNextReturnNotOption => SemanticIssueKind::TypeIterableNextReturnNotOption,
                GateObligationKind::IterableOptionSomeArityMismatch { expected, actual } => {
                    SemanticIssueKind::TypeIterableOptionSomeArityMismatch { expected: *expected, actual: *actual }
                }
                GateObligationKind::InvalidEventInvocationScope => SemanticIssueKind::TypeInvalidEventInvocationScope,
                GateObligationKind::InvalidEventSubscriptionTarget => {
                    SemanticIssueKind::TypeInvalidEventSubscriptionTarget
                }
                GateObligationKind::UnsupportedExpression => SemanticIssueKind::TypeUnsupportedExpression,
                GateObligationKind::MissingTypeAnnotation { name } => {
                    SemanticIssueKind::TypeMissingTypeAnnotation { name: name.to_string() }
                }
                GateObligationKind::InvalidPrimitiveConversionArgument => {
                    SemanticIssueKind::TypeInvalidPrimitiveConversionArgument
                }
                GateObligationKind::InvalidMemberTarget => SemanticIssueKind::TypeInvalidMemberTarget,
                GateObligationKind::UnknownValueType => SemanticIssueKind::TypeUnknownValueType,
                GateObligationKind::UnknownStructType => SemanticIssueKind::TypeUnknownStructType,
                GateObligationKind::UnknownEnumType => SemanticIssueKind::TypeUnknownEnumType,
                GateObligationKind::UnknownStructField { name } => {
                    SemanticIssueKind::TypeUnknownStructField { name: name.to_string() }
                }
                GateObligationKind::UnknownCallTarget => SemanticIssueKind::TypeUnknownCallTarget,
                GateObligationKind::GenericArgumentMismatch { expected, actual } => {
                    SemanticIssueKind::TypeGenericArgumentMismatch { expected: *expected, actual: *actual }
                }
                GateObligationKind::DuplicateLocal { name, previous } => {
                    SemanticIssueKind::ResolveDuplicateLocal { name: name.to_string(), previous: *previous }
                }
                GateObligationKind::GenericBoundNotSatisfied { type_name, contract_name } => {
                    SemanticIssueKind::GenericBoundNotSatisfied {
                        type_name: type_name.to_string(),
                        contract_name: contract_name.to_string(),
                    }
                }
            };
            findings.push(SemanticFinding { kind, site: obligation.site, related: Vec::new() });
        }
    }
    findings
}

/// A proven primitive the legacy checker judges operators and members on. `pointer` is also the
/// ABI representation of every nominal value, `never` never reaches an operator, and `char` and
/// `unit` keep their legacy authority (the query operator model does not classify them).
pub(super) fn judgeable_primitive(ty: SemanticTypeId) -> bool {
    !matches!(ty, SemanticTypeId::POINTER | SemanticTypeId::NEVER | SemanticTypeId::CHAR | SemanticTypeId::UNIT)
}

/// The direct child node of `parent` holding `expression` (the diagnostic site) and its proven
/// primitive type, when the normalized value is proven.
pub(super) fn proven_operand(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    parent: beskid_analysis::syntax::AstNodeId,
    expression: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Expression>,
) -> Option<(beskid_analysis::syntax::AstNodeId, beskid_analysis::syntax::AstNodeId, SemanticTypeId)> {
    let (site, subject) = super::typing::expression_nodes(program, index, parent, expression)?;
    let ty = super::typing::proven_type(db, AstNodeKey { node: subject, ..key })?;
    Some((site, subject, ty))
}
