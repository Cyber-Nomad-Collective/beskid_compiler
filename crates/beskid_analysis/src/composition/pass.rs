use std::collections::HashMap;

use super::{
    collect::{collect, dependency_requests},
    container::ServiceContainer,
    diagnostics::CompositionIssue,
    graph::{build_graph, topo_registration_order},
    host_chain::{build_host_chain, merge_host_registries, resolve_host_key},
    model::{ActivationPlanEntry, BindingPlan, CompositionHookPlan, PluralPlan, ScopeId, ServiceSlot, SingularPlan},
    resolve_inject::resolve_dependency_targets,
    scope_tree::{merge_host_scopes, scope_parent_map, validate_scope_tree},
    snapshot::CompositionSnapshot,
};
use crate::{
    syntax::{AstNodeId, Program, SpanInfo, Spanned},
    syntax_query::{NodeKind, SyntaxSnapshot},
};

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

    let (mut merged_registrations, merge_issues) =
        merge_host_registries(&host_chain, &collected.host_registries, &collected.host_scopes, &merged_scopes);
    issues.extend(merge_issues);
    // Parser `Spanned.id` is deliberately unset. Freeze identities from the same deterministic
    // expanded-tree pre-order that the generation-bound query index uses; a non-unique source
    // span remains invalid and is rejected at the codegen boundary.
    let source_index = SyntaxSnapshot::from_program(input.program, 0);
    for registration in &mut merged_registrations {
        registration.source_node_id = unique_source_node(&source_index, NodeKind::RegistryEntry, registration.span);
    }

    let scope_parents = scope_parent_map(&merged_scopes);
    let container = ServiceContainer::from_registrations(&merged_registrations);
    let requests = dependency_requests(&merged_registrations, &collected.type_inject_fields);

    let registration_scope: HashMap<u32, _> =
        merged_registrations.iter().map(|registration| (registration.id, registration.scope_id)).collect();

    let mut edges = Vec::new();
    let mut plural_registration_ids = Vec::new();
    let mut singular_registration_ids = Vec::new();
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
                    plural_registration_ids.push((
                        request.owner_registration_id,
                        request.span,
                        request.field_node_id,
                        targets.iter().map(|target| target.id).collect::<Vec<_>>(),
                    ));
                } else if let [target] = targets.as_slice() {
                    singular_registration_ids.push((
                        request.owner_registration_id,
                        request.span,
                        request.field_node_id,
                        target.id,
                    ));
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
    let mut singulars = singular_registration_ids
        .into_iter()
        .map(|(owner_registration_id, field_span, field_node_id, target_id)| SingularPlan {
            owner_registration_id,
            field_span,
            field_node_id,
            target_slot: slots[&target_id],
        })
        .collect::<Vec<_>>();
    singulars.sort_by_key(|singular| (singular.owner_registration_id, singular.field_span.start));
    for singular in &mut singulars {
        singular.field_node_id = unique_source_node(&source_index, NodeKind::Field, singular.field_span);
    }
    let mut plurals = plural_registration_ids
        .into_iter()
        .map(|(owner_registration_id, field_span, field_node_id, targets)| PluralPlan {
            owner_registration_id,
            field_span,
            field_node_id,
            target_slots: targets.into_iter().map(|target| slots[&target]).collect(),
        })
        .collect::<Vec<_>>();
    plurals.sort_by_key(|plural| (plural.owner_registration_id, plural.field_span.start));
    for plural in &mut plurals {
        plural.field_node_id = unique_source_node(&source_index, NodeKind::Field, plural.field_span);
    }

    let mut hooks = Vec::new();
    for host in &host_chain {
        let local_scopes = collected.host_scopes.get(&host.name);
        if let Some(host_hooks) = collected.host_hooks.get(&host.name) {
            for hook in host_hooks {
                let scope_id = if hook.scope_id == ScopeId::GLOBAL {
                    Some(ScopeId::GLOBAL)
                } else {
                    local_scopes
                        .and_then(|scopes| scopes.iter().find(|scope| scope.id == hook.scope_id))
                        .and_then(|local| merged_scopes.iter().find(|scope| scope.name == local.name))
                        .map(|scope| scope.id)
                };
                let Some(scope_id) = scope_id else {
                    issues.push(CompositionIssue::UnknownParentScope {
                        scope_name: "<composition hook>".into(),
                        parent_scope: hook.scope_id,
                        span: hook.span,
                    });
                    continue;
                };
                hooks.push(CompositionHookPlan {
                    scope_id,
                    kind: hook.kind,
                    source_node_id: unique_source_node(&source_index, NodeKind::ScopeHook, hook.span),
                    span: hook.span,
                });
            }
        }
    }
    let init_hooks = hooks.iter().filter(|hook| hook.kind == crate::syntax::ScopeHookKind::Init).cloned().collect();
    let startup_hooks =
        hooks.iter().filter(|hook| hook.kind == crate::syntax::ScopeHookKind::Startup).cloned().collect();
    let mut disposal_hooks =
        hooks.into_iter().filter(|hook| hook.kind == crate::syntax::ScopeHookKind::Dispose).collect::<Vec<_>>();
    disposal_hooks.reverse();

    let scope_names = merged_scopes.iter().map(|scope| (scope.id, scope.name.clone())).collect();
    let snapshot = CompositionSnapshot {
        version: 1,
        launched_host,
        source_unit_path: None,
        launch_span: if launch_span == SpanInfo::default() { None } else { Some(launch_span) },
        registrations: merged_registrations.clone(),
        scope_names,
    };
    let plan = BindingPlan {
        launched_host: snapshot.launched_host.clone(),
        activation,
        singulars,
        plurals,
        scope_parents,
        init_hooks,
        startup_hooks,
        disposal_hooks,
    };

    CompositionResult { plan, snapshot, issues, dependency_edges: edges }
}

fn unique_source_node(index: &SyntaxSnapshot<'_>, kind: NodeKind, span: SpanInfo) -> AstNodeId {
    let mut matches = (0..index.len())
        .filter(|id| index.kind_of(*id as u32) == Some(kind) && index.span_of(*id as u32) == Some(span))
        .filter_map(|id| u32::try_from(id).ok());
    match (matches.next(), matches.next()) {
        (Some(id), None) => AstNodeId(id),
        _ => AstNodeId::INVALID,
    }
}

#[cfg(test)]
mod tests {
    use super::{CompositionInput, resolve_composition};
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

    #[test]
    fn freezes_singular_injection_and_declared_lifecycle_hooks() {
        let program = parse_program(
            r#"
type Logger {}
type Worker { inject Logger logger }
host AppHost() {
    registry { single Logger; single Worker; }
    startup() {}
    dispose() {}
}
i32 Main() { launch AppHost(); return 0; }
"#,
        )
        .expect("composition source parses");
        let result = resolve_composition(CompositionInput { program: &program, is_mod_project: false });
        assert!(result.issues.is_empty(), "issues: {:?}", result.issues);
        assert_eq!(result.plan.activation.len(), 2);
        assert_eq!(result.plan.singulars.len(), 1);
        assert_eq!(result.plan.singulars[0].owner_registration_id, result.snapshot.registrations[1].id);
        assert_eq!(result.plan.singulars[0].target_slot.0, 0);
        assert_eq!(result.plan.startup_hooks.len(), 1, "declared startup must not be dropped");
        assert_eq!(result.plan.disposal_hooks.len(), 1, "declared disposal must not be dropped");
    }

    #[test]
    fn freezes_each_plural_injection_field_without_overwriting_siblings() {
        let program = parse_program(
            r#"
type Logger {}
type Worker {
    inject Logger[] primary,
    inject Logger[] secondary
}
host AppHost() { registry { single Logger; single Worker; } }
i32 Main() { launch AppHost(); return 0; }
"#,
        )
        .expect("composition source parses");
        let result = resolve_composition(CompositionInput { program: &program, is_mod_project: false });
        assert!(result.issues.is_empty(), "issues: {:?}", result.issues);
        assert_eq!(result.plan.plurals.len(), 2, "both plural fields require independent plans");
        assert_ne!(result.plan.plurals[0].field_span, result.plan.plurals[1].field_span);
        assert_eq!(result.plan.plurals[0].target_slots, vec![super::ServiceSlot(0)]);
        assert_eq!(result.plan.plurals[1].target_slots, vec![super::ServiceSlot(0)]);
    }
}
