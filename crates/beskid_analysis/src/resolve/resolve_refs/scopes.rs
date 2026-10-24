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

    /// The closest local or item name visible here to the unknown `name` ("did you mean").
    pub(super) fn closest_value_name(&self, name: &str) -> Option<String> {
        let locals = self.local_scopes.iter().flat_map(|scope| scope.keys().map(String::as_str));
        let mut items = Vec::new();
        let mut current = Some(self.current_module);
        while let Some(module_id) = current {
            let Some(module) = self.module_graph.module(module_id) else { break };
            items.extend(module.scope.keys().map(String::as_str));
            current = module.parent;
        }
        crate::analysis::suggest::closest_name(name, locals.chain(items).filter(|candidate| *candidate != "this"))
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
