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

    /// The registrations of `scope_id` whose key is the requested declaration. A request or key
    /// that names no declaration matches nothing.
    pub fn find_scope_matches(
        &self,
        scope_id: ScopeId,
        requested: Option<&super::types::TypeIdentity>,
    ) -> Vec<Registration> {
        let Some(requested) = requested else {
            return Vec::new();
        };
        self.registrations_for_scope(scope_id)
            .iter()
            .filter(|registration| registration.key_identity.as_ref() == Some(requested))
            .cloned()
            .collect()
    }
}
