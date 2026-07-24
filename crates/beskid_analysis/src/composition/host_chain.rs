use std::collections::{HashMap, HashSet};

use super::diagnostics::CompositionIssue;
use super::model::{CompositionHost, Registration, ScopeId};
use crate::syntax::SpanInfo;

pub fn resolve_host_key(hosts: &HashMap<String, CompositionHost>, launch_name: &str) -> Option<String> {
    if hosts.contains_key(launch_name) {
        return Some(launch_name.to_string());
    }
    let tail = launch_name.rsplit('.').next().unwrap_or(launch_name);
    hosts
        .keys()
        .find(|name| name.as_str() == launch_name || name.as_str() == tail || name.ends_with(&format!(".{tail}")))
        .cloned()
}

pub fn build_host_chain<'a>(
    hosts: &'a HashMap<String, CompositionHost>,
    launched_host: &str,
    launch_span: SpanInfo,
) -> Result<Vec<&'a CompositionHost>, CompositionIssue> {
    let launch_key = resolve_host_key(hosts, launched_host).ok_or_else(|| CompositionIssue::UnknownLaunchHost {
        host_name: launched_host.to_string(),
        span: launch_span,
    })?;

    let mut ordered = Vec::new();
    let mut cursor = launch_key;
    let mut seen = HashSet::new();

    loop {
        if !seen.insert(cursor.clone()) {
            let span = hosts.get(&cursor).map(|host| host.span).unwrap_or_default();
            return Err(CompositionIssue::HostInheritanceCycle { host_name: cursor, span });
        }
        let host = hosts
            .get(&cursor)
            .ok_or_else(|| CompositionIssue::UnknownLaunchHost { host_name: cursor.clone(), span: launch_span })?;
        ordered.push(host);
        if let Some(base) = &host.base_host {
            let base_key = resolve_host_key(hosts, base).unwrap_or_else(|| base.clone());
            cursor = base_key;
        } else {
            break;
        }
    }
    ordered.reverse();
    Ok(ordered)
}

pub fn merge_host_registries(
    chain: &[&CompositionHost],
    host_registries: &HashMap<String, Vec<Registration>>,
    host_scopes: &HashMap<String, Vec<super::model::CompositionScope>>,
    merged_scopes: &[super::model::CompositionScope],
) -> (Vec<Registration>, Vec<CompositionIssue>) {
    let scope_name_by_id: HashMap<ScopeId, String> =
        merged_scopes.iter().map(|scope| (scope.id, scope.name.clone())).collect();
    let unified_scope_id: HashMap<String, ScopeId> =
        merged_scopes.iter().map(|scope| (scope.name.clone(), scope.id)).collect();

    let mut merged: Vec<Registration> = Vec::new();
    let mut issues = Vec::new();
    for host in chain {
        let Some(regs) = host_registries.get(&host.name) else {
            continue;
        };
        let local_scopes = host_scopes
            .get(&host.name)
            .map(|scopes| scopes.iter().map(|scope| (scope.id, scope.name.clone())).collect::<HashMap<_, _>>())
            .unwrap_or_default();

        for mut reg in regs.iter().cloned() {
            if reg.scope_id != ScopeId::GLOBAL
                && let Some(scope_name) =
                    local_scopes.get(&reg.scope_id).or_else(|| scope_name_by_id.get(&reg.scope_id))
                && let Some(unified) = unified_scope_id.get(scope_name)
            {
                reg.scope_id = *unified;
            }

            if let Some(existing_index) =
                merged.iter().position(|existing| existing.scope_id == reg.scope_id && existing.key == reg.key)
            {
                let existing = &merged[existing_index];
                if existing.lifetime != reg.lifetime {
                    issues.push(CompositionIssue::OverrideLifetimeMismatch {
                        binding: format!("{:?}", reg.key),
                        span: reg.span,
                    });
                }
                merged[existing_index] = reg;
            } else {
                merged.push(reg);
            }
        }
    }
    (merged, issues)
}
