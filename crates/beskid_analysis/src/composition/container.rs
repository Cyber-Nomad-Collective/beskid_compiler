use std::collections::HashMap;

use super::model::{Registration, ScopeId};

#[derive(Debug, Clone, Default)]
pub struct ServiceContainer {
    by_scope: HashMap<ScopeId, Vec<Registration>>,
}

impl ServiceContainer {
    pub fn from_registrations(registrations: &[Registration]) -> Self {
        let mut by_scope: HashMap<ScopeId, Vec<Registration>> = HashMap::new();
        for registration in registrations {
            by_scope.entry(registration.scope_id).or_default().push(registration.clone());
        }
        Self { by_scope }
    }

    pub fn registrations_for_scope(&self, scope_id: ScopeId) -> &[Registration] {
        self.by_scope.get(&scope_id).map(Vec::as_slice).unwrap_or_default()
    }

    pub fn find_scope_matches(&self, scope_id: ScopeId, requested: &str) -> Vec<Registration> {
        self.registrations_for_scope(scope_id)
            .iter()
            // Keys and requests compare by their last path segment, so `Hosting.CommandLine` and
            // `CommandLine` (through `use`) name the same service.
            .filter(|registration| last_segment(registration.key.name()) == last_segment(requested))
            .cloned()
            .collect()
    }
}

fn last_segment(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}
