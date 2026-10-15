//! The `exchange(place, move value)` builtin (`language-meta--memory-model--memory-and-references`,
//! "Destructive read of a field"). `exchange` is not a keyword: a single-segment call named
//! `exchange` with two arguments and no lexical binding of that name is the builtin. It stores
//! `value` into `place` and evaluates to the previous value, so its type is the place's type.

use super::super::*;

/// Lowering hook for the integration: load `place`, store `value` into it through the barriered
/// field (or element) store, and yield the loaded value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ExchangeCall {
    pub call: AstNodeKey,
    pub place: AstNodeKey,
    pub value: AstNodeKey,
}

pub(in crate::semantic_contract) fn exchange_call_for_node(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
) -> Option<ExchangeCall> {
    let call = index.node_at(program, key.node)?.of::<beskid_analysis::syntax::CallExpression>()?;
    let beskid_analysis::syntax::Expression::Path(path) = &call.callee.node else { return None };
    let [segment] = path.node.path.node.segments.as_slice() else { return None };
    if segment.node.name.node.name != "exchange" || !segment.node.type_args.is_empty() || call.args.len() != 2 {
        return None;
    }
    if resolve_lexical_declaration(program, index, key.node, "exchange").is_some() {
        return None;
    }
    let argument = |position: usize| {
        index
            .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(&call.args[position]))
            .map(|node| AstNodeKey { node: normalized_expression_node(index, node), ..key })
    };
    Some(ExchangeCall { call: key, place: argument(0)?, value: argument(1)? })
}

/// The `exchange` builtin call at `key`, if `key` is one.
pub fn exchange_call(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ExchangeCall> {
    let Some(syntax) = db.syntax_unit(key.unit) else { return Ok(None) };
    with_node(db, syntax, key, |program, index, _node| exchange_call_for_node(program, index, key))
}
