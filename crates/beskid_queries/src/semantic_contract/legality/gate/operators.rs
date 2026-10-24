//! Operator and numeric-literal obligations (E1209, E1210, binary E1206, literal E1206, T0905).
//!
//! Mirrors the legacy `type_binary_expression` and `type_unary_expression`: operands are judged
//! only when both are proven primitives that the legacy checker classifies
//! (`judgeable_primitive`); two numeric primitives promote to one operand type as in
//! `promote_binary_numeric_operands`. Numeric literals follow contextual literal typing (see
//! [`collect_literal_obligations`]): the one [`expected_type`] authority supplies the context.

use super::super::typing::{is_numeric, nodes_of_kind};
use crate::semantic_contract::calls::generic_source_expression_identity;
use crate::semantic_contract::model::GenericSourceTypeIdentity;
use super::*;
use beskid_analysis::syntax::{
    BinaryExpression, BinaryOp, LetStatement, Literal, PrimitiveType, UnaryExpression, UnaryOp,
    float_literal_magnitude, float_literal_primitive_type, integer_literal_fits_primitive, integer_literal_magnitude,
    integer_literal_primitive_type,
};
use beskid_analysis::syntax_query::DynNodeRef;

pub(super) fn collect_operator_obligations(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<GateObligation>,
) {
    for node in nodes_of_kind(index, key.node, NodeKind::BinaryExpression) {
        let Some(binary) = index.node_at(program, node).and_then(|node| node.of::<BinaryExpression>()) else {
            continue;
        };
        let Some((_, left_subject, left)) = proven_operand(db, program, index, key, node, binary.left.as_ref()) else {
            continue;
        };
        let Some((_, right_subject, right)) = proven_operand(db, program, index, key, node, binary.right.as_ref())
        else {
            continue;
        };
        let site = AstNodeKey { node, ..key };
        // A contract value (a box, v0.6.7) has no operators: its identity is the implementing
        // value's, which no operator reads in this release.
        if [left_subject, right_subject].into_iter().any(|subject| {
            matches!(
                generic_source_expression_identity(db, AstNodeKey { node: subject, ..key }),
                Ok(GenericSourceTypeIdentity::Contract { .. })
            )
        }) {
            findings.push(GateObligation { kind: GateObligationKind::InvalidBinaryOp, site });
            continue;
        }
        if !judgeable_primitive(left) || !judgeable_primitive(right) {
            continue;
        }
        let op = binary.op.node;
        if op == BinaryOp::Add && (left == SemanticTypeId::STRING || right == SemanticTypeId::STRING) {
            continue;
        }
        let promoted = is_numeric(left) && is_numeric(right);
        if !promoted && left != right {
            findings
                .push(GateObligation { kind: GateObligationKind::OperandMismatch { expected: left, actual: right }, site });
            continue;
        }
        let legal = match op {
            BinaryOp::Or | BinaryOp::And => left == SemanticTypeId::BOOL,
            BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::Shl | BinaryOp::Shr => primitive_integer(left),
            BinaryOp::IdentityEq | BinaryOp::IdentityNotEq => left == SemanticTypeId::STRING,
            BinaryOp::Eq | BinaryOp::NotEq => {
                is_numeric(left) || left == SemanticTypeId::BOOL || left == SemanticTypeId::STRING
            }
            // Strings order by their UTF-8 bytes (Unicode code point order) through `str_cmp`.
            BinaryOp::Lt | BinaryOp::Lte | BinaryOp::Gt | BinaryOp::Gte => {
                is_numeric(left) || left == SemanticTypeId::BOOL || left == SemanticTypeId::STRING
            }
            BinaryOp::Add => is_numeric(left) || left == SemanticTypeId::STRING,
            BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => is_numeric(left),
        };
        if !legal {
            findings.push(GateObligation { kind: GateObligationKind::InvalidBinaryOp, site });
        }
    }
    for node in nodes_of_kind(index, key.node, NodeKind::UnaryExpression) {
        let Some(unary) = index.node_at(program, node).and_then(|node| node.of::<UnaryExpression>()) else {
            continue;
        };
        // A negated integer literal is one literal; `collect_literal_obligations` judges it.
        if unary.op.node == UnaryOp::Neg
            && let beskid_analysis::syntax::Expression::Literal(value) = &unary.expr.node
            && matches!(value.node.literal.node, Literal::Integer(_))
        {
            continue;
        }
        let Some((_, _, operand)) = proven_operand(db, program, index, key, node, unary.expr.as_ref()) else {
            continue;
        };
        if !judgeable_primitive(operand) {
            continue;
        }
        let legal = match unary.op.node {
            UnaryOp::Neg => is_numeric(operand),
            UnaryOp::Not => operand == SemanticTypeId::BOOL,
        };
        if !legal {
            findings.push(GateObligation { kind: GateObligationKind::InvalidUnaryOp, site: AstNodeKey { node, ..key } });
        }
    }
}

/// Numeric literals against their own primitive and their context (rule T1, contextual literal
/// typing). Every position [`expected_type`] gives a type is a context: a declared local, an
/// assignment target, a call or method argument (an inferred generic parameter included), an enum
/// payload, a struct field, a `return`, an array element, a match arm, a value block, and the peer
/// of a binary operand.
///
/// * An unsuffixed integer literal (a directly negated one is the negative value, judged at the
///   unary expression) takes the primitive integer of its context and must fit it (T0905); a
///   negative value never fits an unsigned type. Without a context it has its default type
///   (`i32`, else `i64`, else `word` for a hexadecimal bit pattern), which only an inferred `let`
///   initializer has to fit.
/// * An unsuffixed float literal takes `f32` from its context, `f64` otherwise, and must be
///   finite in it (T0905).
/// * A suffix fixes the literal's type and wins over the context; a suffix whose type differs
///   from a numeric context is a type mismatch (E1206) at the literal, never a conversion, and so
///   is a float literal in an integer context. Binary operands keep the operator's numeric
///   promotion, so neither applies to them.
pub(super) fn collect_literal_obligations(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<GateObligation>,
) {
    for node in nodes_of_kind(index, key.node, NodeKind::Literal) {
        let Some(literal) = index.node_at(program, node).and_then(|node| node.of::<Literal>()) else { continue };
        match literal {
            Literal::Integer(text) => judge_integer_literal(db, program, index, key, node, text, findings),
            Literal::Float(text) => judge_float_literal(db, index, key, node, text, findings),
            _ => {}
        }
    }
    collect_constant_obligations(db, program, index, key, findings);
}

fn judge_integer_literal(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    node: beskid_analysis::syntax::AstNodeId,
    text: &str,
    findings: &mut Vec<GateObligation>,
) {
    let negation = negating_unary(program, index, node);
    // The value root is the negation when there is one: the whole `-literal` takes the context.
    let root = negation.or_else(|| parent_node(index, node));
    let expected = root.and_then(|root| literal_expected_type(db, key, root));
    let site = AstNodeKey { node: negation.unwrap_or(node), ..key };
    let primitive = if integer_literal_magnitude(text) == text {
        match expected.as_ref().and_then(|expected| integer_primitive_for_semantic(expected.abi_type())) {
            Some(primitive) => primitive,
            // An inferred `let` takes the literal's default type, so the literal must fit it (a
            // decimal above `i64::MAX` has no default integer type).
            None if root.is_some_and(|root| inferred_let_initializer(program, index, root)) => {
                integer_literal_primitive_type(text)
            }
            None => return,
        }
    } else {
        let own = integer_literal_primitive_type(text);
        if let Some(expected) = expected.as_ref().and_then(numeric_value_context)
            && expected != semantic_type_for_primitive(own)
        {
            findings.push(GateObligation {
                kind: GateObligationKind::LiteralTypeMismatch { expected, actual: semantic_type_for_primitive(own) },
                site,
            });
            return;
        }
        own
    };
    let value = match negation {
        Some(_) => format!("-{text}"),
        None => text.to_string(),
    };
    if !integer_literal_fits_primitive(&value, primitive) {
        findings.push(GateObligation {
            kind: GateObligationKind::NumericLiteralOutOfRange { value: Arc::from(value), primitive: primitive_name(primitive) },
            site,
        });
    }
}

fn judge_float_literal(
    db: &dyn Db,
    index: &SyntaxIndex,
    key: AstNodeKey,
    node: beskid_analysis::syntax::AstNodeId,
    text: &str,
    findings: &mut Vec<GateObligation>,
) {
    let site = AstNodeKey { node, ..key };
    let suffixed = text.ends_with("_f32") || text.ends_with("_f64");
    let own = semantic_type_for_primitive(float_literal_primitive_type(text));
    let expected = parent_node(index, node).and_then(|root| literal_expected_type(db, key, root));
    if let Some(expected) = expected.as_ref().and_then(numeric_value_context)
        && (primitive_integer(expected) || (suffixed && expected != own))
    {
        findings.push(GateObligation { kind: GateObligationKind::LiteralTypeMismatch { expected, actual: own }, site });
        return;
    }
    let primitive = match expected.map(|expected| expected.abi_type()) {
        Some(SemanticTypeId::F32) if !suffixed => PrimitiveType::F32,
        _ => float_literal_primitive_type(text),
    };
    let magnitude = float_literal_magnitude(text);
    let valid = match primitive {
        PrimitiveType::F32 => magnitude.parse::<f32>().is_ok_and(|value| value.is_finite()),
        _ => magnitude.parse::<f64>().is_ok_and(|value| value.is_finite()),
    };
    if !valid {
        findings.push(GateObligation {
            kind: GateObligationKind::NumericLiteralOutOfRange {
                value: Arc::from(magnitude),
                primitive: primitive_name(primitive),
            },
            site,
        });
    }
}

/// The context of the literal value rooted at `root` (its `LiteralExpression`, or the negating
/// `UnaryExpression`), when one exists.
fn literal_expected_type(
    db: &dyn Db,
    key: AstNodeKey,
    root: beskid_analysis::syntax::AstNodeId,
) -> Option<ExpectedType> {
    expected_type(db, AstNodeKey { node: root, ..key }).ok().flatten()
}

/// The numeric primitive a value context requires of a literal whose type is already fixed (by a
/// suffix, or by being a float). A binary operand follows the operator's numeric promotion
/// instead, and a non-numeric context is judged by the typing obligations.
fn numeric_value_context(expected: &ExpectedType) -> Option<SemanticTypeId> {
    if matches!(expected.context, ExpectedTypeContext::BinaryOperand { .. } | ExpectedTypeContext::ExpressionStatement { .. }) {
        return None;
    }
    let GenericSourceTypeIdentity::Abi(ty) = &expected.identity else { return None };
    is_numeric(*ty).then_some(*ty)
}

fn primitive_name(primitive: PrimitiveType) -> Arc<str> {
    Arc::from(format!("{primitive:?}").to_lowercase())
}

/// Module constant uses at an integer context their declared literal does not fit
/// (`i32 x = LIMIT;` with `const LIMIT = 3000000000;`). A constant use behaves as its literal,
/// so it is judged exactly as an unsuffixed literal in that context and reported at the use
/// site, never left to fail as a missing lowering fact.
fn collect_constant_obligations(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<GateObligation>,
) {
    for node in nodes_of_kind(index, key.node, NodeKind::PathExpression) {
        let site = AstNodeKey { node, ..key };
        let Ok(Some(text)) = constant_integer_literal(db, site) else { continue };
        let Some(primitive) = literal_expected_type(db, key, node)
            .and_then(|expected| integer_primitive_for_semantic(expected.abi_type()))
        else {
            continue;
        };
        if !integer_literal_fits_primitive(&text, primitive) {
            let name = index
                .node_at(program, node)
                .and_then(|node| node.of::<beskid_analysis::syntax::PathExpression>())
                .map(|path| {
                    path.path.node.segments.iter().map(|segment| segment.node.name.node.name.as_str()).collect::<Vec<_>>().join(".")
                })
                .unwrap_or_else(|| text.to_string());
            findings.push(GateObligation {
                kind: GateObligationKind::NumericLiteralOutOfRange { value: Arc::from(name), primitive: primitive_name(primitive) },
                site,
            });
        }
    }
}

/// Whether `root` (a literal value root) is the initializer of a `let` without a type annotation.
fn inferred_let_initializer(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    root: beskid_analysis::syntax::AstNodeId,
) -> bool {
    let mut child = root;
    let Some(mut parent) = parent_node(index, child) else { return false };
    while matches!(index.kind(parent), Some(NodeKind::Expression | NodeKind::GroupedExpression)) {
        child = parent;
        let Some(next) = parent_node(index, child) else { return false };
        parent = next;
    }
    if index.kind(parent) != Some(NodeKind::LetStatement) {
        return false;
    }
    let Some(binding) = index.node_at(program, parent).and_then(|node| node.of::<LetStatement>()) else {
        return false;
    };
    binding.type_annotation.is_none()
        && index.direct_child_id(program, parent, DynNodeRef::from(&binding.value)) == Some(child)
}

/// The `-` unary expression whose direct operand is the literal expression owning `literal`
/// (`Literal` -> `LiteralExpression` -> `Expression` -> `UnaryExpression`), if any.
fn negating_unary(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    literal: beskid_analysis::syntax::AstNodeId,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    let expression = parent_node(index, literal)?;
    if index.kind(expression) != Some(NodeKind::LiteralExpression) {
        return None;
    }
    let mut candidate = parent_node(index, expression)?;
    if index.kind(candidate) == Some(NodeKind::Expression) {
        candidate = parent_node(index, candidate)?;
    }
    let unary = index.node_at(program, candidate)?.of::<UnaryExpression>()?;
    (unary.op.node == UnaryOp::Neg
        && matches!(&unary.expr.node, beskid_analysis::syntax::Expression::Literal(value)
            if matches!(value.node.literal.node, Literal::Integer(_))))
    .then_some(candidate)
}
