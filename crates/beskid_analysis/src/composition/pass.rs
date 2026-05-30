use std::collections::HashMap;

use crate::hir::HirProgram;
use crate::syntax::SpanInfo;
use crate::syntax::Spanned;

use super::collect::{collect, dependency_requests};
use super::container::ServiceContainer;
use super::diagnostics::CompositionIssue;
use super::graph::{build_graph, topo_registration_order};
use super::host_chain::{build_host_chain, merge_host_registries, resolve_host_key};
use super::model::BindingPlan;
use super::resolve_inject::resolve_dependency_targets;
use super::scope_tree::{merge_host_scopes, scope_parent_map, validate_scope_tree};
use super::snapshot::CompositionSnapshot;

#[derive(Clone)]
pub struct CompositionInput<'a> {
    pub program: &'a Spanned<HirProgram>,
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
                issues.push(CompositionIssue::MissingLaunchHost {
                    span: Some(input.program.span),
                });
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
        if !merged_scopes
            .iter()
            .any(|scope| scope.name == with_site.scope_name)
        {
            issues.push(CompositionIssue::WithArgsMismatch {
                scope_name: with_site.scope_name.clone(),
                span: with_site.span,
            });
        }
    }

    let (merged_registrations, merge_issues) = merge_host_registries(
        &host_chain,
        &collected.host_registries,
        &collected.host_scopes,
        &merged_scopes,
    );
    issues.extend(merge_issues);

    let scope_parents = scope_parent_map(&merged_scopes);
    let container = ServiceContainer::from_registrations(&merged_registrations);
    let requests = dependency_requests(&merged_registrations, &collected.type_inject_fields);

    let registration_scope: HashMap<u32, _> = merged_registrations
        .iter()
        .map(|registration| (registration.id, registration.scope_id))
        .collect();

    let mut edges = Vec::new();
    let mut plural_bindings = HashMap::new();
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
                    plural_bindings.insert(
                        request.owner_registration_id,
                        targets.iter().map(|target| target.id).collect(),
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
        .unwrap_or_else(|| {
            resolve_host_key(&collected.hosts, &launch_host).unwrap_or(launch_host)
        });

    let scope_names = merged_scopes
        .iter()
        .map(|scope| (scope.id, scope.name.clone()))
        .collect();
    let snapshot = CompositionSnapshot {
        version: 1,
        launched_host,
        launch_span: if launch_span == SpanInfo::default() {
            None
        } else {
            Some(launch_span)
        },
        registrations: merged_registrations.clone(),
        scope_names,
    };
    let plan = BindingPlan {
        registration_order,
        plural_bindings,
        scope_parents,
    };

    CompositionResult {
        plan,
        snapshot,
        issues,
        dependency_edges: edges,
    }
}
