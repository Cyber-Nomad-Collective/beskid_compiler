//! Operator and numeric-literal obligations (E1209, E1210, binary E1206, T0905).
//!
//! Mirrors the legacy `type_binary_expression`, `type_unary_expression`, and
//! `type_id_for_literal`: operands are judged only when both are proven primitives that the
//! legacy checker classifies (`judgeable_primitive`); two numeric primitives promote to one
//! operand type as in `promote_binary_numeric_operands`. Unsuffixed integer literals take their
//! primitive from context and keep their legacy authority; suffixed integers and float
//! literals are closed.

use super::super::typing::{is_numeric, nodes_of_kind};
use super::*;
use beskid_analysis::syntax::{
    BinaryExpression, BinaryOp, Literal, UnaryExpression, UnaryOp, float_literal_magnitude,
    float_literal_primitive_type, integer_literal_fits_primitive, integer_literal_magnitude,
    integer_literal_primitive_type,
};

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
        let Some((_, _, left)) = proven_operand(db, program, index, key, node, binary.left.as_ref()) else { continue };
        let Some((_, _, right)) = proven_operand(db, program, index, key, node, binary.right.as_ref()) else {
            continue;
        };
        if !judgeable_primitive(left) || !judgeable_primitive(right) {
            continue;
        }
        let site = AstNodeKey { node, ..key };
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
            BinaryOp::Lt | BinaryOp::Lte | BinaryOp::Gt | BinaryOp::Gte => is_numeric(left) || left == SemanticTypeId::BOOL,
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

/// Suffixed integer literals (including a directly negated one, judged as the negative value at
/// the unary expression exactly as the legacy checker does) and float literals outside their
/// primitive's range.
pub(super) fn collect_literal_obligations(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<GateObligation>,
) {
    for node in nodes_of_kind(index, key.node, NodeKind::Literal) {
        let Some(literal) = index.node_at(program, node).and_then(|node| node.of::<Literal>()) else { continue };
        match literal {
            Literal::Integer(text) => {
                if integer_literal_magnitude(text) == text {
                    // Unsuffixed: the primitive comes from the destination type (legacy
                    // `contextual_expected_type`), which is not proven here.
                    continue;
                }
                let primitive = integer_literal_primitive_type(text);
                let (fits, site) = match negating_unary(program, index, node) {
                    Some(unary) => (integer_literal_fits_primitive(&format!("-{text}"), primitive), unary),
                    None => (integer_literal_fits_primitive(text, primitive), node),
                };
                if !fits {
                    findings.push(GateObligation {
                        kind: GateObligationKind::NumericLiteralOutOfRange {
                            primitive: Arc::from(format!("{primitive:?}").to_lowercase()),
                        },
                        site: AstNodeKey { node: site, ..key },
                    });
                }
            }
            Literal::Float(text) => {
                let primitive = float_literal_primitive_type(text);
                let magnitude = float_literal_magnitude(text);
                let valid = match primitive {
                    beskid_analysis::syntax::PrimitiveType::F32 => {
                        magnitude.parse::<f32>().is_ok_and(|value| value.is_finite())
                    }
                    _ => magnitude.parse::<f64>().is_ok_and(|value| value.is_finite()),
                };
                if !valid {
                    findings.push(GateObligation {
                        kind: GateObligationKind::NumericLiteralOutOfRange {
                            primitive: Arc::from(format!("{primitive:?}").to_lowercase()),
                        },
                        site: AstNodeKey { node, ..key },
                    });
                }
            }
            _ => {}
        }
    }
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
