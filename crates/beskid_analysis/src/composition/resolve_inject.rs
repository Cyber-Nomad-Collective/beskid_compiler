use std::collections::HashMap;

use crate::syntax::InjectQualifier;

use super::container::ServiceContainer;
use super::diagnostics::CompositionIssue;
use super::model::{InjectDependency, Registration, ScopeId};

pub fn resolve_dependency_targets(
    dependency: &InjectDependency,
    request_scope: ScopeId,
    scope_parents: &HashMap<ScopeId, Option<ScopeId>>,
    container: &ServiceContainer,
) -> Result<Vec<Registration>, CompositionIssue> {
    let chain = walk_chain(request_scope, scope_parents);
    let scoped_chain = match dependency.qualifier {
        Some(InjectQualifier::Global) => vec![ScopeId::GLOBAL],
        Some(InjectQualifier::Parent) => chain.into_iter().skip(1).collect(),
        None => chain,
    };

    for scope in scoped_chain {
        let matches = container.find_scope_matches(scope, dependency.requested_identity.as_ref());
        if dependency.is_plural {
            if !matches.is_empty() {
                return Ok(matches);
            }
        } else if matches.len() == 1 {
            return Ok(matches);
        } else if matches.len() > 1 {
            return Err(CompositionIssue::AmbiguousInject {
                requested_type: dependency.requested_type.clone(),
                span: dependency.span,
                field_name: dependency.field_name.clone(),
                name_span: dependency.name_span,
                type_span: dependency.type_span,
                // Related places are shown in the field's own file; registrations elsewhere are not.
                candidates: matches
                    .iter()
                    .filter(|registration| registration.unit == dependency.field_unit)
                    .map(|registration| registration.span)
                    .collect(),
            });
        }
    }

    if dependency.is_plural {
        Err(CompositionIssue::EmptyPluralInject {
            requested_type: dependency.requested_type.clone(),
            span: dependency.span,
        })
    } else {
        Err(CompositionIssue::UnresolvedInject {
            requested_type: dependency.requested_type.clone(),
            span: dependency.span,
        })
    }
}

pub fn walk_chain(mut scope_id: ScopeId, scope_parents: &HashMap<ScopeId, Option<ScopeId>>) -> Vec<ScopeId> {
    let mut chain = vec![scope_id];
    while let Some(Some(parent)) = scope_parents.get(&scope_id) {
        chain.push(*parent);
        scope_id = *parent;
    }
    if !chain.contains(&ScopeId::GLOBAL) {
        chain.push(ScopeId::GLOBAL);
    }
    chain
}
