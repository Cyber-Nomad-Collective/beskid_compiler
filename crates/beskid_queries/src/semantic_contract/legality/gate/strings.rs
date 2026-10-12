//! String concatenation and interpolation obligations (E1209).
//!
//! `"count: ${value}"` desugars to `+` on strings, so both forms are judged here. Lowering
//! converts a `string`, `bool`, `f32`, `f64`, `i8`, `i16`, `i32`, `i64`, `u8`, `u16` or `u32`
//! operand to text; any other proven operand type (a nominal, array or function value, `char`,
//! `u64`, `word`, `unit`) has no text form, and without this obligation it reached emission as
//! an internal error. An operand whose ABI type is unproven is never a finding. Operands are judged
//! by their value ABI (not `node_type`, which does not classify nominal values), so a nominal,
//! array or function operand is seen as the pointer it is.

use super::super::typing::{expression_nodes, nodes_of_kind};
use super::*;
use beskid_analysis::syntax::{BinaryExpression, BinaryOp};

pub(super) fn collect_string_concat_obligations(
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
        if binary.op.node != BinaryOp::Add {
            continue;
        }
        let Some(left) = operand_abi(db, program, index, key, node, binary.left.as_ref()) else { continue };
        let Some(right) = operand_abi(db, program, index, key, node, binary.right.as_ref()) else { continue };
        if left.2 != SemanticTypeId::STRING && right.2 != SemanticTypeId::STRING {
            continue;
        }
        for (site, subject, ty) in [left, right] {
            if has_text_form(ty) {
                continue;
            }
            let subject = AstNodeKey { node: subject, ..key };
            let found = generic_source_expression_identity(db, subject)
                .map(|identity| identity.display_name())
                .unwrap_or_else(|_| ty.display_name());
            findings.push(GateObligation {
                kind: GateObligationKind::StringConcatOperandNotConvertible { found: Arc::from(found) },
                site: AstNodeKey { node: site, ..key },
            });
        }
    }
}

/// The direct child node holding `expression` (the diagnostic site), its normalized value node and
/// that value's ABI type, when proven.
fn operand_abi(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    parent: beskid_analysis::syntax::AstNodeId,
    expression: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Expression>,
) -> Option<(beskid_analysis::syntax::AstNodeId, beskid_analysis::syntax::AstNodeId, SemanticTypeId)> {
    let (site, subject) = expression_nodes(program, index, parent, expression)?;
    let ty = abi_type(db, AstNodeKey { node: subject, ..key }).ok().flatten()?;
    Some((site, subject, ty))
}

/// The proven operand types string concatenation converts to text.
fn has_text_form(ty: SemanticTypeId) -> bool {
    matches!(
        ty,
        SemanticTypeId::STRING
            | SemanticTypeId::BOOL
            | SemanticTypeId::F32
            | SemanticTypeId::F64
            | SemanticTypeId::I8
            | SemanticTypeId::I16
            | SemanticTypeId::I32
            | SemanticTypeId::I64
            | SemanticTypeId::U8
            | SemanticTypeId::U16
            | SemanticTypeId::U32
    )
}
