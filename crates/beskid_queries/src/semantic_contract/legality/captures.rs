//! Lambda stack-reference capture legality (E1233).
//!
//! A lambda captures each outer local by value into its closure environment. A `mut` local or a
//! native pointer is a stack reference: snapshotting it would silently stop aliasing the source
//! storage, so the closure static plan rejects it. This fact reports that as a user error instead
//! of an unavailable closure fact. Spawn operands are judged by `spawn_legality` (E1225) and are
//! skipped here.

use super::*;

/// Every non-spawn lambda in the item `key` that captures a stack reference, reported once per
/// lambda for its first such capture, in node order.
pub(super) fn lambda_capture_findings(db: &dyn Db, key: AstNodeKey) -> Vec<SemanticFinding> {
    let Some(syntax) = db.syntax_unit(key.unit) else { return Vec::new() };
    let lambdas = match with_node(db, syntax, key, |_program, index, _node| {
        let mut lambdas = Vec::new();
        collect_nodes_of_kind(index, key.node, NodeKind::LambdaExpression, &mut lambdas);
        Some(lambdas.into_iter().filter(|lambda| !is_spawn_operand(index, *lambda)).collect::<Vec<_>>())
    }) {
        Ok(Some(lambdas)) => lambdas,
        Ok(None) | Err(_) => return Vec::new(),
    };
    lambdas
        .into_iter()
        .map(|node| AstNodeKey { node, ..key })
        .filter_map(|lambda| {
            let environment = closure_environment(db, lambda).ok().flatten()?;
            let capture =
                environment.captures.iter().find(|capture| capture.class == CaptureStorageClass::StackReference)?;
            let name = with_node(db, syntax, capture.declaration, |_program, _index, node| {
                node.of::<beskid_analysis::syntax::Identifier>().map(|identifier| identifier.name.clone())
            })
            .ok()
            .flatten()?;
            Some(SemanticFinding {
                kind: SemanticIssueKind::LambdaCapturesStackReference { name },
                site: lambda,
                related: Vec::new(),
            })
        })
        .collect()
}

fn is_spawn_operand(index: &SyntaxIndex, lambda: beskid_analysis::syntax::AstNodeId) -> bool {
    let mut current = lambda;
    while let Some(parent) = parent_node(index, current) {
        match index.kind(parent) {
            Some(NodeKind::Expression | NodeKind::GroupedExpression) => current = parent,
            Some(NodeKind::SpawnExpression) => return true,
            _ => return false,
        }
    }
    false
}
