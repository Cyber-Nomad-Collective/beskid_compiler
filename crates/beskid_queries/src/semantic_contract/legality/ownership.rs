//! Affine ownership findings (E1234, E1239-E1243) of `affine_ownership`, reported per callable
//! of the item, including the lambdas it contains.

use super::*;

fn callables_of(db: &dyn Db, key: AstNodeKey) -> Vec<AstNodeKey> {
    let Some(syntax) = db.syntax_unit(key.unit) else { return Vec::new() };
    match with_node(db, syntax, key, |_program, index, _node| {
        let mut found = Vec::new();
        for kind in [NodeKind::FunctionDefinition, NodeKind::MethodDefinition, NodeKind::TestDefinition, NodeKind::LambdaExpression] {
            collect_nodes_of_kind(index, key.node, kind, &mut found);
        }
        found.sort();
        found.dedup();
        Some(found)
    }) {
        Ok(Some(found)) => found.into_iter().map(|node| AstNodeKey { node, ..key }).collect(),
        Ok(None) | Err(_) => Vec::new(),
    }
}

pub(super) fn affine_findings(db: &dyn Db, item: AstNodeKey) -> Vec<SemanticFinding> {
    let mut findings = Vec::new();
    for callable in callables_of(db, item) {
        let Ok(Some(ownership)) = affine_ownership(db, callable) else { continue };
        for finding in ownership.findings.iter() {
            let name = finding.name.clone();
            let (kind, related) = match finding.kind {
                AffineFindingKind::UseAfterMove => (
                    SemanticIssueKind::UseAfterMove { name },
                    finding.related.map(|related| (related, "moved here")),
                ),
                AffineFindingKind::MoveOfNonOwningName => {
                    (SemanticIssueKind::MoveOfNonOwningName { name, class: finding.detail.clone() }, None)
                }
                AffineFindingKind::MoveInLoopWithoutReassignment => {
                    (SemanticIssueKind::MoveInLoopWithoutReassignment { name }, None)
                }
                AffineFindingKind::BorrowEscapes => (
                    SemanticIssueKind::BorrowEscapes { name },
                    finding.related.map(|related| (related, "borrowed here")),
                ),
                AffineFindingKind::ImplicitAliasOfMovedValue => (SemanticIssueKind::ImplicitAliasOfMovedValue { name }, None),
                AffineFindingKind::DestructiveReadWithoutEmpty => (
                    SemanticIssueKind::DestructiveReadWithoutEmpty {
                        field: name,
                        type_name: finding.detail.clone().unwrap_or_default(),
                    },
                    None,
                ),
            };
            findings.push(SemanticFinding { kind, site: finding.site, related: related.into_iter().collect() });
        }
    }
    findings
}
