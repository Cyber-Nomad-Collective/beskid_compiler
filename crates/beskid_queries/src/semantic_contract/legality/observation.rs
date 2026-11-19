//! Source checks for retained property observation callbacks.

use super::*;
use crate::semantic_contract::contract_observation::conforming_properties;
use crate::semantic_contract::properties::{
    ObservationDependencyKind, checked_observation_dependencies, observation_equality_instance,
    observation_has_bounded_equality, observation_value_identity,
};
use beskid_analysis::syntax::{AstNodeId, Expression, ObserveStatement, Program, ReturnStatement, Spanned};
use beskid_analysis::syntax_query::DynNodeRef;

pub(super) fn observation_findings(db: &dyn Db, item: AstNodeKey) -> Vec<SemanticFinding> {
    let Some(syntax) = db.syntax_unit(item.unit).filter(|syntax| syntax.accepts_key(db, item)) else {
        return Vec::new();
    };
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    index
        .ids_of_kind(NodeKind::ObserveStatement)
        .filter(|node| is_ancestor(index, item.node, *node))
        .filter_map(|node| check_observation(db, program, index, AstNodeKey { node, ..item }))
        .collect()
}

fn check_observation(
    db: &dyn Db,
    program: &Spanned<Program>,
    index: &SyntaxIndex,
    site: AstNodeKey,
) -> Option<SemanticFinding> {
    let observation = index.node_at(program, site.node)?.of::<ObserveStatement>()?;
    let Expression::Member(member) = &observation.target.node else { return None };
    let name = &member.node.member.node.name;
    let target = index.direct_child_id(program, site.node, DynNodeRef::from(&observation.target))?;
    let target = AstNodeKey { node: normalized_expression_node(index, target), ..site };
    let invalid_target = || SemanticFinding {
        kind: SemanticIssueKind::TypeInvalidObservationTarget { name: name.clone() },
        site: target,
        related: Vec::new(),
    };
    let (declarations, identity, bounded_equality) = match contract_observation(db, site) {
        Ok(Some(property)) => {
            let Ok(conformers) = conforming_properties(db, site, &property) else { return Some(invalid_target()) };
            let mut declarations = conformers.into_iter().map(|(_, declaration)| declaration).collect::<Vec<_>>();
            declarations.sort_by(|left, right| (left.unit.path(db), left.node).cmp(&(right.unit.path(db), right.node)));
            if declarations.is_empty() {
                return Some(invalid_target());
            }
            (declarations, Some(property.identity), false)
        }
        Ok(None) => {
            let Some(property) = observation_target(db, site).ok().flatten() else { return Some(invalid_target()) };
            let identity = observation_value_identity(db, target, None).ok();
            let bounded_equality = identity.is_none() && observation_has_bounded_equality(db, site, property);
            (vec![property.declaration], identity, bounded_equality)
        }
        Err(_) => return Some(invalid_target()),
    };
    let equality = bounded_equality
        || identity.as_ref().is_some_and(|identity| {
            has_primitive_equality(identity)
                || observation_equality_instance(db, site, identity).ok().flatten().is_some()
        });
    for declaration in declarations {
        if let Err(error) = checked_observation_dependencies(db, declaration) {
            let kind = match error.kind {
                ObservationDependencyKind::Opaque => {
                    SemanticIssueKind::TypeOpaqueObservationDependency { name: name.clone() }
                }
                ObservationDependencyKind::Cycle => {
                    SemanticIssueKind::TypeObservationDependencyCycle { name: name.clone() }
                }
            };
            return Some(SemanticFinding {
                kind,
                site: error.site,
                related: vec![(target, "observation registered here".into())],
            });
        }
        let always = property_always(db, declaration).ok().flatten().unwrap_or(false);
        if !always && !equality {
            return Some(SemanticFinding {
                kind: SemanticIssueKind::TypeObservationEqualityUnavailable { name: name.clone() },
                site: target,
                related: vec![(declaration, "property declared here".into())],
            });
        }
    }
    let callback = index.direct_child_id(program, site.node, DynNodeRef::from(&observation.callback))?;
    let callback = normalized_expression_node(index, callback);
    let returned_value = invalid_callback_return(db, program, index, site, callback)?;
    Some(SemanticFinding {
        kind: SemanticIssueKind::TypeInvalidObservationCallback,
        site: AstNodeKey { node: returned_value, ..site },
        related: Vec::new(),
    })
}

fn has_primitive_equality(identity: &GenericSourceTypeIdentity) -> bool {
    let GenericSourceTypeIdentity::Abi(ty) = *identity else {
        return false;
    };
    super::super::calls::primitive_integer(ty)
        || matches!(
            ty,
            SemanticTypeId::BOOL
                | SemanticTypeId::CHAR
                | SemanticTypeId::STRING
                | SemanticTypeId::F32
                | SemanticTypeId::F64
        )
}

fn invalid_callback_return(
    db: &dyn Db,
    program: &Spanned<Program>,
    index: &SyntaxIndex,
    site: AstNodeKey,
    callback: AstNodeId,
) -> Option<AstNodeId> {
    let mut returns = Vec::new();
    collect_nodes_of_kind(index, callback, NodeKind::ReturnStatement, &mut returns);
    returns.into_iter().find_map(|statement| {
        if nearest_ancestor(index, statement, |kind| kind == NodeKind::LambdaExpression) != Some(callback) {
            return None;
        }
        let value = index.node_at(program, statement)?.of::<ReturnStatement>()?.value.as_ref()?;
        let expression = index.direct_child_id(program, statement, DynNodeRef::from(value))?;
        let expression = normalized_expression_node(index, expression);
        let value_type = node_type(db, AstNodeKey { node: expression, ..site });
        (!matches!(value_type, Ok(Some(SemanticTypeId::UNIT | SemanticTypeId::NEVER)))).then_some(expression)
    })
}
