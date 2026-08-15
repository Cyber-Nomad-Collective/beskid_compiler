use std::collections::HashMap;

use crate::syntax::{Program, SpanInfo, Spanned};

use super::collect::{collect, dependency_requests};
use super::container::ServiceContainer;
use super::diagnostics::CompositionIssue;
use super::graph::{build_graph, topo_registration_order};
use super::host_chain::{build_host_chain, merge_host_registries, resolve_host_key};
use super::model::{ActivationPlanEntry, BindingPlan, PluralPlan, ServiceSlot};
use super::resolve_inject::resolve_dependency_targets;
use super::scope_tree::{merge_host_scopes, scope_parent_map, validate_scope_tree};
use super::snapshot::CompositionSnapshot;

#[derive(Clone)]
pub struct CompositionInput<'a> {
    /// Expanded syntax is the composition authority; this pass does not require syntax.
    pub program: &'a Spanned<Program>,
    pub is_mod_project: bool,
}

#[derive(Debug, Clone)]
pub struct CompositionResult {
    pub plan: BindingPlan,
    pub snapshot: CompositionSnapshot,
    pub issues: Vec<CompositionIssue>,
    pub dependency_edges: Vec<(u32, u32)>,
}

pub fn resolve_composition(input: CompositionInput<'_>) -> CompositionResult {
    let collected = collect(input.program);
    let mut issues = Vec::new();

    if input.is_mod_project {
        for host in collected.hosts.values() {
            issues.push(CompositionIssue::HostInModProject { span: host.span });
        }
    }

    let (launch_host, launch_span) = match collected.launches.as_slice() {
        [] => {
            if !collected.hosts.is_empty() {
                issues.push(CompositionIssue::MissingLaunchHost { span: Some(input.program.span) });
            }
            (String::new(), SpanInfo::default())
        }
        [single] => (single.host_name.clone(), single.span),
        [first, second, ..] => {
            issues.push(CompositionIssue::MultipleLaunchHosts {
                first_span: Some(first.span),
                second_span: Some(second.span),
            });
            (first.host_name.clone(), first.span)
        }
    };

    let host_chain = if launch_host.is_empty() {
        Vec::new()
    } else {
        match build_host_chain(&collected.hosts, &launch_host, launch_span) {
            Ok(chain) => chain,
            Err(issue) => {
                issues.push(issue);
                Vec::new()
            }
        }
    };

    let merged_scopes = merge_host_scopes(&host_chain, &collected.host_scopes);
    issues.extend(validate_scope_tree(&merged_scopes));
    for with_site in &collected.with_sites {
        if !merged_scopes.iter().any(|scope| scope.name == with_site.scope_name) {
            issues.push(CompositionIssue::WithArgsMismatch {
                scope_name: with_site.scope_name.clone(),
                span: with_site.span,
            });
        }
    }

    let (merged_registrations, merge_issues) =
        merge_host_registries(&host_chain, &collected.host_registries, &collected.host_scopes, &merged_scopes);
    issues.extend(merge_issues);

    let scope_parents = scope_parent_map(&merged_scopes);
    let container = ServiceContainer::from_registrations(&merged_registrations);
    let requests = dependency_requests(&merged_registrations, &collected.type_inject_fields);

    let registration_scope: HashMap<u32, _> =
        merged_registrations.iter().map(|registration| (registration.id, registration.scope_id)).collect();

    let mut edges = Vec::new();
    let mut plural_registration_ids = HashMap::new();
    for request in requests {
        let request_scope = registration_scope
            .get(&request.owner_registration_id)
            .copied()
            .unwrap_or(crate::composition::model::ScopeId::GLOBAL);
        match resolve_dependency_targets(&request, request_scope, &scope_parents, &container) {
            Ok(targets) => {
                for target in &targets {
                    // Build dependency -> dependent edges so topo order is init-safe.
                    edges.push((target.id, request.owner_registration_id));
                }
                if request.is_plural {
                    plural_registration_ids.insert(
                        request.owner_registration_id,
                        targets.iter().map(|target| target.id).collect::<Vec<_>>(),
                    );
                }
            }
            Err(issue) => issues.push(issue),
        }
    }

    let registration_order = match build_graph(&merged_registrations, &edges) {
        Ok(dag) => topo_registration_order(&dag),
        Err(issue) => {
            issues.push(issue);
            Vec::new()
        }
    };

    let launched_host = host_chain
        .last()
        .map(|host| host.name.clone())
        .unwrap_or_else(|| resolve_host_key(&collected.hosts, &launch_host).unwrap_or(launch_host));

    let activation = registration_order
        .into_iter()
        .enumerate()
        .map(|(slot, registration_id)| ActivationPlanEntry {
            registration_id,
            slot: ServiceSlot(u32::try_from(slot).expect("composition registration count exceeds u32")),
        })
        .collect::<Vec<_>>();
    let slots = activation.iter().map(|entry| (entry.registration_id, entry.slot)).collect::<HashMap<_, _>>();
    let mut plurals = plural_registration_ids
        .into_iter()
        .map(|(owner_registration_id, targets)| PluralPlan {
            owner_registration_id,
            target_slots: targets.into_iter().map(|target| slots[&target]).collect(),
        })
        .collect::<Vec<_>>();
    plurals.sort_by_key(|plural| plural.owner_registration_id);

    let scope_names = merged_scopes.iter().map(|scope| (scope.id, scope.name.clone())).collect();
    let snapshot = CompositionSnapshot {
        version: 1,
        launched_host,
        launch_span: if launch_span == SpanInfo::default() { None } else { Some(launch_span) },
        registrations: merged_registrations.clone(),
        scope_names,
    };
    let plan = BindingPlan { activation, plurals, scope_parents };

    CompositionResult { plan, snapshot, issues, dependency_edges: edges }
}

#[cfg(test)]
mod tests {
    use super::{resolve_composition, CompositionInput};
    use crate::services::parse_program;

    #[test]
    fn resolves_composition_from_expanded_syntax() {
        let program = parse_program(
            r#"
host AppHost() {
    registry {
        single Logger;
    }
}

i32 Main() {
    launch AppHost();
    return 0;
}
"#,
        )
        .expect("composition source parses");

        let result = resolve_composition(CompositionInput { program: &program, is_mod_project: false });

        assert_eq!(result.snapshot.launched_host, "AppHost");
        assert_eq!(result.plan.activation.len(), 1);
        assert_eq!(result.plan.activation[0].registration_id, result.snapshot.registrations[0].id);
        assert_eq!(result.plan.activation[0].slot.0, 0);
        assert!(result.plan.plurals.is_empty());
        assert!(result.issues.is_empty(), "issues: {:?}", result.issues);
    }
}
