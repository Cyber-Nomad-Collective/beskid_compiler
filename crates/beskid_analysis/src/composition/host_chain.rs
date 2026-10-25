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
) -> (Vec<Registration>, Vec<(super::model::UnitPath, CompositionIssue)>) {
    let scope_name_by_id: HashMap<ScopeId, String> =
        merged_scopes.iter().map(|scope| (scope.id, scope.name.clone())).collect();
    let unified_scope_id: HashMap<String, ScopeId> =
        merged_scopes.iter().map(|scope| (scope.name.clone(), scope.id)).collect();

    let mut merged: Vec<Registration> = Vec::new();
    // The host (index in `chain`) that contributed each merged entry. Only a later host
    // overrides a key; several registrations of one key inside one host are all kept, and a
    // singular inject of that key is then ambiguous (E1705).
    let mut owners: Vec<usize> = Vec::new();
    let mut issues = Vec::new();
    for (host_index, host) in chain.iter().enumerate() {
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

            // The same implementation for the same key twice in one host and scope is a mistake:
            // each line creates its own instance (E1724). Distinct implementations are kept.
            if let Some(first) = merged.iter().enumerate().find_map(|(index, existing)| {
                (owners[index] == host_index
                    && existing.scope_id == reg.scope_id
                    && same_key(existing, &reg)
                    && existing.implementation_identity.is_some()
                    && existing.implementation_identity == reg.implementation_identity)
                    .then_some(existing)
            }) {
                issues.push((
                    reg.unit.clone(),
                    CompositionIssue::DuplicateRegistration {
                        implementation: reg.implementation.clone(),
                        key: reg.key.name().to_string(),
                        span: reg.span,
                        first_span: (first.unit == reg.unit).then_some(first.span),
                    },
                ));
            }
            let overridden = merged
                .iter()
                .enumerate()
                .filter(|(index, existing)| {
                    owners[*index] < host_index && existing.scope_id == reg.scope_id && same_key(existing, &reg)
                })
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            if let Some(&first) = overridden.first() {
                let existing = &merged[first];
                if existing.lifetime != reg.lifetime {
                    issues.push((
                        reg.unit.clone(),
                        CompositionIssue::OverrideLifetimeMismatch {
                            binding: reg.key.name().to_string(),
                            span: reg.span,
                            lifetime: reg.lifetime.keyword(),
                            // A base registration in another unit has no place in this file.
                            overridden_span: (existing.unit == reg.unit).then_some(existing.span),
                            overridden_lifetime: existing.lifetime.keyword(),
                        },
                    ));
                }
                // The derived host replaces every base registration of the key at its position.
                merged[first] = reg;
                owners[first] = host_index;
                for index in overridden.into_iter().skip(1).rev() {
                    merged.remove(index);
                    owners.remove(index);
                }
            } else {
                merged.push(reg);
                owners.push(host_index);
            }
        }
    }
    (merged, issues)
}

/// Keys compare by kind and by the declaration they name; a key that names no declaration never
/// overrides.
fn same_key(left: &Registration, right: &Registration) -> bool {
    use super::model::RegistrationKey::{Contract, SelfType};
    matches!((&left.key, &right.key), (Contract(_), Contract(_)) | (SelfType(_), SelfType(_)))
        && left.key_identity.is_some()
        && left.key_identity == right.key_identity
}
