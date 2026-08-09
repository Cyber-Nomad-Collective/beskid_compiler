use std::collections::HashMap;

use crate::syntax;

use super::super::errors::{ResolveError, ResolveWarning};
use super::super::ids::LocalId;
use super::super::resolver::Resolver;

impl Resolver {
    pub(super) fn insert_generic(&mut self, name: &str) {
        let scope = match self.generic_scopes.last_mut() {
            Some(scope) => scope,
            None => return,
        };
        scope.insert(name.to_string(), ());
    }

    pub(super) fn is_generic(&self, name: &str) -> bool {
        self.generic_scopes.iter().rev().any(|scope| scope.contains_key(name))
    }

    pub(super) fn resolve_local(&self, name: &str) -> Option<LocalId> {
        for scope in self.local_scopes.iter().rev() {
            if let Some(local) = scope.get(name).copied() {
                return Some(local);
            }
        }
        None
    }

    pub(super) fn insert_local(&mut self, name: &str, span: syntax::SpanInfo) {
        if let Some((_, previous_span)) = self.find_shadowed_local(name) {
            self.warnings.push(ResolveWarning::ShadowedLocal { name: name.to_string(), span, previous: previous_span });
        } else if let Some(previous_item) = self.resolve_item_in_scope(name) {
            let previous_span = self.items.get(previous_item.0).map(|item| item.span).unwrap_or(span);
            self.warnings.push(ResolveWarning::ShadowedLocal { name: name.to_string(), span, previous: previous_span });
        }
        let scope = match self.local_scopes.last_mut() {
            Some(scope) => scope,
            None => return,
        };
        if let Some(prev) = scope.get(name).copied() {
            let previous = self.tables.local_info(prev).map(|info| info.span).unwrap_or(span);
            self.errors.push(ResolveError::DuplicateLocal { name: name.to_string(), span, previous });
            return;
        }
        let id = self.tables.intern_local(name.to_string(), span, self.current_source_path.clone());
        scope.insert(name.to_string(), id);
    }

    pub(super) fn find_shadowed_local(&self, name: &str) -> Option<(LocalId, syntax::SpanInfo)> {
        for scope in self.local_scopes.iter().rev().skip(1) {
            if let Some(local) = scope.get(name).copied() {
                let span = self.tables.local_info(local).map(|info| info.span).unwrap_or_else(|| syntax::SpanInfo {
                    start: 0,
                    end: 0,
                    line_col_start: (1, 1),
                    line_col_end: (1, 1),
                });
                return Some((local, span));
            }
        }
        None
    }

    pub(super) fn push_scope(&mut self) {
        self.local_scopes.push(HashMap::new());
    }

    pub(super) fn pop_scope(&mut self) {
        self.local_scopes.pop();
    }

    pub(super) fn push_generic_scope(&mut self) {
        self.generic_scopes.push(HashMap::new());
    }

    pub(super) fn pop_generic_scope(&mut self) {
        self.generic_scopes.pop();
    }
}
