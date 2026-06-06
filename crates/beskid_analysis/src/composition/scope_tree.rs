use std::collections::HashMap;

use super::diagnostics::CompositionIssue;
use super::model::{CompositionHost, CompositionScope, ScopeId};

pub fn merge_host_scopes(
    chain: &[&CompositionHost],
    host_scopes: &HashMap<String, Vec<CompositionScope>>,
) -> Vec<CompositionScope> {
    let mut by_name: HashMap<String, CompositionScope> = HashMap::new();
    let mut name_to_id: HashMap<String, ScopeId> = HashMap::new();
    let mut next_id = 1_u32;

    for host in chain {
        let Some(scopes) = host_scopes.get(&host.name) else {
            continue;
        };
        let local: HashMap<ScopeId, &CompositionScope> =
            scopes.iter().map(|scope| (scope.id, scope)).collect();

        for scope in scopes {
            let parent_name = scope.parent.and_then(|parent_id| {
                if parent_id == ScopeId::GLOBAL {
                    None
                } else {
                    local.get(&parent_id).map(|parent| parent.name.as_str())
                }
            });

            let unified_id = *name_to_id.entry(scope.name.clone()).or_insert_with(|| {
                let id = ScopeId(next_id);
                next_id += 1;
                id
            });

            let unified_parent = parent_name.map(|name| {
                *name_to_id.entry(name.to_string()).or_insert_with(|| {
                    let id = ScopeId(next_id);
                    next_id += 1;
                    id
                })
            });

            by_name.insert(
                scope.name.clone(),
                CompositionScope {
                    id: unified_id,
                    name: scope.name.clone(),
                    parent: unified_parent,
                    span: scope.span,
                },
            );
        }
    }

    by_name.into_values().collect()
}

pub fn validate_scope_tree(scopes: &[CompositionScope]) -> Vec<CompositionIssue> {
    let mut issues = Vec::new();
    let mut names = HashMap::<String, ScopeId>::new();
    for scope in scopes {
        if let Some(previous) = names.insert(scope.name.clone(), scope.id) {
            issues.push(CompositionIssue::DuplicateScopeName {
                scope_name: scope.name.clone(),
                span: scope.span,
                first_scope: previous,
                second_scope: scope.id,
            });
        }
        if let Some(parent) = scope.parent {
            let parent_exists = parent == ScopeId::GLOBAL || scopes.iter().any(|s| s.id == parent);
            if !parent_exists {
                issues.push(CompositionIssue::UnknownParentScope {
                    scope_name: scope.name.clone(),
                    parent_scope: parent,
                    span: scope.span,
                });
            }
        }
    }
    issues
}

pub fn scope_parent_map(scopes: &[CompositionScope]) -> HashMap<ScopeId, Option<ScopeId>> {
    let mut parents = HashMap::new();
    parents.insert(ScopeId::GLOBAL, None);
    for scope in scopes {
        parents.insert(scope.id, scope.parent);
    }
    parents
}
