//! Operator and numeric-literal obligations (E1209, E1210, binary E1206, T0905).
//!
//! Mirrors the legacy `type_binary_expression`, `type_unary_expression`, and
//! `type_id_for_literal`: operands are judged only when both are proven primitives that the
//! legacy checker classifies (`judgeable_primitive`); two numeric primitives promote to one
//! operand type as in `promote_binary_numeric_operands`. Suffixed integers and float literals
//! are judged against their own primitive. An unsuffixed integer literal takes its primitive
//! from the legacy `contextual_expected_type` and is judged only at a surface-declared
//! destination that proves a primitive integer (`contextual_literal_primitive`).

use super::super::typing::{enclosing_callable, is_numeric, nodes_of_kind};
use super::*;
use beskid_analysis::syntax::{
    BinaryExpression, BinaryOp, CallExpression, Expression, FunctionDefinition, LetStatement, Literal,
    MethodDefinition, PrimitiveType, ReturnStatement, UnaryExpression, UnaryOp, float_literal_magnitude,
    float_literal_primitive_type, integer_literal_fits_primitive, integer_literal_magnitude,
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

/// Integer literals (including a directly negated one, judged as the negative value at the unary
/// expression exactly as the legacy `type_unary_expression` does) and float literals outside
/// their primitive's range. A suffixed literal is judged against its suffix; an unsuffixed one
/// against the primitive integer of its proven destination (legacy `type_id_for_literal` under
/// `contextual_expected_type`), and not at all without one.
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
            Literal::Integer(text) => {
                let unsuffixed = integer_literal_magnitude(text) == text;
                let negation = negating_unary(program, index, node);
                let primitive = if unsuffixed {
                    // The value root is the negation when there is one: the legacy checker types
                    // the whole `-literal` under the destination's contextual type.
                    let root = negation.or_else(|| parent_node(index, node));
                    match root.and_then(|root| literal_destination_primitive(db, program, index, key, root)) {
                        Some(primitive) => primitive,
                        // An inferred `let` takes the literal's natural type, so the literal must
                        // fit it (a decimal above `i64::MAX` has no natural integer type).
                        None if root.is_some_and(|root| inferred_let_initializer(program, index, root)) => {
                            integer_literal_primitive_type(text)
                        }
                        None => continue,
                    }
                } else {
                    integer_literal_primitive_type(text)
                };
                let (fits, site) = match negation {
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
    collect_constant_obligations(db, program, index, key, findings);
}

/// Module constant uses at a surface-declared integer destination their declared literal does
/// not fit (`i32 x = LIMIT;` with `const LIMIT = 3000000000;`). A constant use behaves as its
/// literal, so it is judged exactly as an unsuffixed literal at that destination and reported
/// at the use site, never left to fail as a missing lowering fact.
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
        let Some(primitive) = literal_destination_primitive(db, program, index, key, node) else { continue };
        if !integer_literal_fits_primitive(&text, primitive) {
            findings.push(GateObligation {
                kind: GateObligationKind::NumericLiteralOutOfRange {
                    primitive: Arc::from(format!("{primitive:?}").to_lowercase()),
                },
                site,
            });
        }
    }
}

/// The primitive integer an unsuffixed literal or constant use rooted at `root` takes: its
/// surface-declared destination, else (rule T1) the type of the other operand of the binary
/// expression it is an operand of, a call result included. Two bare operands share the type
/// their binary expression takes from its own context. No promotion to a wider type exists.
fn literal_destination_primitive(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    root: beskid_analysis::syntax::AstNodeId,
) -> Option<PrimitiveType> {
    contextual_literal_primitive(db, program, index, key, root).or_else(|| {
        let expected = expected_type(db, AstNodeKey { node: root, ..key }).ok()??;
        matches!(expected.context, ExpectedTypeContext::BinaryOperand { .. })
            .then(|| integer_primitive_for_semantic(expected.abi_type()))
            .flatten()
    })
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

/// The primitive integer the legacy `contextual_expected_type` gives the unsuffixed integer
/// value rooted at `root` (its `LiteralExpression`, or the negating `UnaryExpression`), when the
/// value is, up to `Expression` and grouping wrappers, directly one of the surface-declared
/// destinations the legacy checker types it under:
///
/// * the initializer of an annotated `let` (`type_statement`, `Statement::Let`);
/// * the value of a `return` owned by the item, against the declared result
///   (`type_statement`, `Statement::Return`, with `current_return_type`);
/// * an argument of a direct call to a non-generic function without `bulk` parameters, against
///   the declared parameter (`type_argument_with_expected`).
///
/// A destination that is not a primitive integer gives no contextual primitive (the legacy
/// checker then falls back to the literal's own `i64`), and is not judged here.
/// `contextual_integer_literal_abi_type` cannot serve: it proves a destination only for a
/// literal that fits it.
fn contextual_literal_primitive(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    root: beskid_analysis::syntax::AstNodeId,
) -> Option<PrimitiveType> {
    let mut child = root;
    let mut parent = parent_node(index, child)?;
    while matches!(index.kind(parent), Some(NodeKind::Expression | NodeKind::GroupedExpression)) {
        child = parent;
        parent = parent_node(index, child)?;
    }
    let expected = match index.kind(parent)? {
        NodeKind::LetStatement => {
            let binding = index.node_at(program, parent)?.of::<LetStatement>()?;
            if index.direct_child_id(program, parent, DynNodeRef::from(&binding.value))? != child {
                return None;
            }
            semantic_type_from_syntax(&binding.type_annotation.as_ref()?.node).ok()?
        }
        NodeKind::ReturnStatement => {
            let statement = index.node_at(program, parent)?.of::<ReturnStatement>()?;
            if index.direct_child_id(program, parent, DynNodeRef::from(statement.value.as_ref()?))? != child {
                return None;
            }
            if enclosing_callable(index, parent)? != key.node {
                return None;
            }
            let item = index.node_at(program, key.node)?;
            let declared = if let Some(function) = item.of::<FunctionDefinition>() {
                function.return_type.as_ref()
            } else {
                item.of::<MethodDefinition>()?.return_type.as_ref()
            };
            semantic_type_from_syntax(&declared?.node).ok()?
        }
        NodeKind::CallExpression => {
            let call = index.node_at(program, parent)?.of::<CallExpression>()?;
            if !matches!(call.callee.node, Expression::Path(_))
                || index.direct_child_id(program, parent, DynNodeRef::from(call.callee.as_ref()))? == child
            {
                return None;
            }
            let call_key = AstNodeKey { node: parent, ..key };
            let Ok(Some(CallLowering::Direct(declaration))) = call_lowering(db, call_key) else { return None };
            let declaration_syntax =
                db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration))?;
            let declaration_index = declaration_syntax.syntax_index(db);
            let declaration_program = declaration_syntax.expanded_program(db);
            let function =
                declaration_index.node_at(declaration_program, declaration.node)?.of::<FunctionDefinition>()?;
            if !function.generics.is_empty() || function.parameters.iter().any(|parameter| parameter.node.bulk) {
                return None;
            }
            let arguments = call_arguments(db, call_key).ok()??;
            if arguments.len() != function.parameters.len() {
                return None;
            }
            let subject = normalized_expression_node(index, child);
            let position = arguments
                .iter()
                .position(|argument| argument.node == child || normalized_expression_node(index, argument.node) == subject)?;
            semantic_type_from_syntax(&function.parameters[position].node.ty.node).ok()?
        }
        _ => return None,
    };
    Some(match expected {
        SemanticTypeId::I8 => PrimitiveType::I8,
        SemanticTypeId::I16 => PrimitiveType::I16,
        SemanticTypeId::I32 => PrimitiveType::I32,
        SemanticTypeId::I64 => PrimitiveType::I64,
        SemanticTypeId::U8 => PrimitiveType::U8,
        SemanticTypeId::U16 => PrimitiveType::U16,
        SemanticTypeId::U32 => PrimitiveType::U32,
        SemanticTypeId::U64 => PrimitiveType::U64,
        SemanticTypeId::WORD => PrimitiveType::Word,
        _ => return None,
    })
}
