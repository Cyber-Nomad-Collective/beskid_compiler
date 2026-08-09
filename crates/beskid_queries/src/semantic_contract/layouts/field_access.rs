//! Canonical semantic layout implementation.

use super::super::*;

#[salsa::tracked]
pub(in crate::semantic_contract) fn aggregate_field_access_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<AggregateFieldAccess> {
    with_node(db, syntax, key, |program, index, node| {
        let path = node.of::<beskid_analysis::syntax::PathExpression>()?;
        let [receiver, field] = path.path.node.segments.as_slice() else {
            return None;
        };
        if !receiver.node.type_args.is_empty() || !field.node.type_args.is_empty() {
            return None;
        }
        let (declaration, receiver) =
            nominal_local_receiver_declaration(db, program, index, key, receiver.node.name.node.name.as_str())?;
        let layout = aggregate_layout(db, declaration).ok().flatten()?;
        let index = layout
            .fields
            .iter()
            .position(|(name, _)| name.as_ref() == field.node.name.node.name)
            .and_then(|index| u32::try_from(index).ok())?;
        Some(Ok(AggregateFieldAccess { declaration, receiver, index }))
    })?
    .transpose()
}

/// Resolve the smallest generation-safe member receiver: an unqualified local with an explicit
/// nominal parameter or let annotation. Calls, inferred locals, and chained receivers remain
/// unavailable rather than reconstructing retired HIR type information.
pub(in crate::semantic_contract) fn nominal_local_receiver_declaration(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    receiver_name: &str,
) -> Option<(AstNodeKey, AstNodeKey)> {
    let local = resolve_lexical_declaration(program, index, key.node, receiver_name)?;
    let receiver = AstNodeKey { node: local, ..key };
    let parent = parent_node(index, local)?;
    let annotation = match index.kind(parent)? {
        beskid_analysis::syntax_query::NodeKind::Parameter => index
            .node_at(program, parent)?
            .of::<beskid_analysis::syntax::Parameter>()
            .map(|parameter| &parameter.ty.node),
        beskid_analysis::syntax_query::NodeKind::LetStatement => index
            .node_at(program, parent)?
            .of::<beskid_analysis::syntax::LetStatement>()
            .and_then(|statement| statement.type_annotation.as_ref())
            .map(|annotation| &annotation.node),
        _ => None,
    }?;
    let beskid_analysis::syntax::Type::Complex(path) = annotation else {
        return None;
    };
    resolve_type_declaration(db, key, &path.node).map(|declaration| (declaration, receiver))
}
