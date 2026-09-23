//! Enum-pattern binding types, ABI types, and specializations.

use super::super::*;
use super::*;

pub(super) fn join_match_arm_type(
    current: Option<SemanticTypeId>,
    candidate: SemanticTypeId,
) -> Result<Option<SemanticTypeId>, SemanticError> {
    match current {
        None => Ok(Some(candidate)),
        Some(previous) if previous == candidate => Ok(Some(previous)),
        Some(SemanticTypeId::NEVER) => Ok(Some(candidate)),
        Some(previous) if candidate == SemanticTypeId::NEVER => Ok(Some(previous)),
        Some(_) => Err(SemanticError::unavailable("node_type")),
    }
}

pub(in crate::semantic_contract) fn pattern_binding_semantic_type(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    node: beskid_analysis::syntax_query::DynNodeRef<'_>,
) -> Option<Result<SemanticTypeId, SemanticError>> {
    let path = node.of::<beskid_analysis::syntax::PathExpression>()?;
    let [segment] = path.path.node.segments.as_slice() else {
        return None;
    };
    if !segment.node.type_args.is_empty() {
        return None;
    }
    let declaration = resolve_lexical_declaration(program, index, key.node, segment.node.name.node.name.as_str())?;
    pattern_binding_abi_type(db, index, key, declaration)
}

/// Derive the ABI representation of an enum-pattern binding from its already-resolved match
/// layout. Both ordinary expression typing and generic-call specialization consume this fact.
pub(crate) fn pattern_binding_abi_type(
    db: &dyn Db,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    declaration: beskid_analysis::syntax::AstNodeId,
) -> Option<Result<SemanticTypeId, SemanticError>> {
    let binding = match pattern_binding_fact(db, index, key, declaration)? {
        Ok(binding) => binding,
        Err(error) => return Some(Err(error)),
    };
    Some(Ok(match binding.payload {
        AggregateFieldShape::Scalar(semantic) => semantic,
        AggregateFieldShape::Nominal(_) => SemanticTypeId::POINTER,
    }))
}

pub(in crate::semantic_contract) fn pattern_binding_fact(
    db: &dyn Db,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    declaration: beskid_analysis::syntax::AstNodeId,
) -> Option<Result<EnumMatchBindingFact, SemanticError>> {
    pattern_binding_fact_in_environment(db, index, key, declaration, None)
}

/// Resolve an enum-pattern binding through the immutable specialization of the enclosing item.
///
/// A generic outer match can provide the source identity of a nested binding even when its
/// target ABI is only a pointer. The ordinary fact deliberately remains target-neutral, while
/// this path is available only to a concrete item specialization.
pub(in crate::semantic_contract) fn pattern_binding_fact_in_environment(
    db: &dyn Db,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    declaration: beskid_analysis::syntax::AstNodeId,
    enclosing: Option<&Arc<[GenericSubstitution]>>,
) -> Option<Result<EnumMatchBindingFact, SemanticError>> {
    if index.kind(parent_node(index, declaration)?)? != beskid_analysis::syntax_query::NodeKind::Pattern {
        return None;
    }
    let arm = nearest_ancestor(index, declaration, |kind| kind == beskid_analysis::syntax_query::NodeKind::MatchArm)?;
    let outer_match =
        nearest_ancestor(index, arm, |kind| kind == beskid_analysis::syntax_query::NodeKind::MatchExpression)?;
    if outer_match == key.node {
        return None;
    }
    let outer_match = AstNodeKey { node: outer_match, ..key };
    let query = match enclosing {
        Some(enclosing) => enum_match_specialization(db, outer_match, enclosing.clone()),
        None => enum_match(db, outer_match),
    };
    let fact = match query {
        Ok(Some(fact)) => fact,
        Ok(None) | Err(_) => return Some(Err(SemanticError::unavailable("pattern_binding"))),
    };
    fact.arms
        .iter()
        .filter_map(|arm| enum_match_pattern_binding(&arm.pattern, declaration))
        .find(|binding| binding.declaration.node == declaration)
        .map(Ok)
        .or_else(|| Some(Err(SemanticError::unavailable("pattern_binding"))))
}

/// Return the exact specialized fact for a path which resolves to an enum-pattern binding.
///
/// This is deliberately keyed by the path use rather than a synthetic binding identity, so the
/// generation-safe local resolver remains the sole authority for lexical scope.
pub fn pattern_binding_specialization(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Arc<[GenericSubstitution]>,
) -> SemanticQueryResult<EnumMatchBindingFact> {
    let syntax = db
        .syntax_unit(key.unit)
        .filter(|syntax| syntax.accepts_key(db, key))
        .ok_or_else(|| SemanticError::unavailable("pattern_binding_specialization"))?;
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let path = index
        .node_at(program, key.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::PathExpression>())
        .ok_or_else(|| SemanticError::unavailable("pattern_binding_specialization"))?;
    let [segment] = path.path.node.segments.as_slice() else {
        return Ok(None);
    };
    if !segment.node.type_args.is_empty() {
        return Ok(None);
    }
    let declaration = resolve_lexical_declaration(program, index, key.node, segment.node.name.node.name.as_str())
        .ok_or_else(|| SemanticError::unavailable("pattern_binding_specialization"))?;
    pattern_binding_fact_in_environment(db, index, key, declaration, Some(&enclosing)).transpose()
}

fn enum_match_pattern_binding(
    pattern: &EnumMatchPatternFact,
    declaration: beskid_analysis::syntax::AstNodeId,
) -> Option<EnumMatchBindingFact> {
    match pattern {
        EnumMatchPatternFact::Binding(binding) if binding.declaration.node == declaration => Some(binding.clone()),
        EnumMatchPatternFact::Enum(pattern) => {
            pattern.items.iter().find_map(|item| enum_match_pattern_binding(item, declaration))
        }
        EnumMatchPatternFact::Wildcard
        | EnumMatchPatternFact::Binding(_)
        | EnumMatchPatternFact::UnitLiteral { .. }
        | EnumMatchPatternFact::ScalarLiteral(_) => None,
    }
}
