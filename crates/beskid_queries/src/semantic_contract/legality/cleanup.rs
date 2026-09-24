//! Scoped-cleanup (E1230) and dead-growth (E1231) legality findings.
//!
//! Both facts existed before the gate (`scoped_cleanup`, `dead_collection_growth`) and were judged
//! eagerly for every unit of an assembly while `build_typed_program` admitted it, so an invalid
//! scoped `use` or a dead growth in an item nothing reachable ever lowers poisoned every request.
//! They are now evaluated only for the items `check_items` judges, like every other legality fact.

use super::*;

/// Every scoped `use` in the item `key` whose scoped-cleanup fact carries a rejection, in node
/// order. A scoped `use` whose fact is unavailable is a compiler gap, not a finding.
pub(super) fn scoped_cleanup_findings(db: &dyn Db, key: AstNodeKey) -> Vec<SemanticFinding> {
    item_nodes_of_kind(db, key, NodeKind::ScopedUseStatement)
        .into_iter()
        .filter_map(|site| {
            let diagnostic = scoped_cleanup(db, site).ok().flatten()?.diagnostic?;
            Some(SemanticFinding {
                kind: SemanticIssueKind::ScopedCleanupRejected { reason: format!("{diagnostic:?}") },
                site,
                related: Vec::new(),
            })
        })
        .collect()
}

/// Every discarded canonical growth call in the item `key` whose `mut T[]` parameter owner the
/// body never publishes (`BSP-REQ-35580A7D7B75`), in node order.
pub(super) fn dead_growth_findings(db: &dyn Db, key: AstNodeKey) -> Vec<SemanticFinding> {
    let Some(syntax) = db.syntax_unit(key.unit) else { return Vec::new() };
    let candidates = match with_node(db, syntax, key, |program, index, _node| {
        let mut calls = Vec::new();
        collect_nodes_of_kind(index, key.node, NodeKind::CallExpression, &mut calls);
        Some(calls.into_iter().filter(|call| is_growth_call_candidate(program, index, *call)).collect::<Vec<_>>())
    }) {
        Ok(Some(candidates)) => candidates,
        Ok(None) | Err(_) => return Vec::new(),
    };
    candidates
        .into_iter()
        .map(|node| AstNodeKey { node, ..key })
        .filter(|call| matches!(dead_collection_growth(db, *call), Ok(Some(_))))
        .map(|site| SemanticFinding { kind: SemanticIssueKind::DeadCollectionGrowth, site, related: Vec::new() })
        .collect()
}

fn item_nodes_of_kind(db: &dyn Db, key: AstNodeKey, kind: NodeKind) -> Vec<AstNodeKey> {
    let Some(syntax) = db.syntax_unit(key.unit) else { return Vec::new() };
    match with_node(db, syntax, key, |_program, index, _node| {
        let mut found = Vec::new();
        collect_nodes_of_kind(index, key.node, kind, &mut found);
        Some(found)
    }) {
        Ok(Some(found)) => found.into_iter().map(|node| AstNodeKey { node, ..key }).collect(),
        Ok(None) | Err(_) => Vec::new(),
    }
}
