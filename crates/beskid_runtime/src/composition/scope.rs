//! Active scope stack for the native dependency-injection container.
//!
//! Each `with <scope>` activation pushes an `ActiveScope` onto the stack so that:
//!
//! * service instances created while the scope is live are tracked in registration order;
//! * `composition_scope_leave` disposes those instances in reverse order (LIFO), as required
//!   by the language-meta `composition / dependency-injection` spec;
//! * resolution can walk the active scope stack from innermost to outermost when looking up
//!   scoped services.

use super::registry::{RegistrationId, ScopeId};

/// State for one active `with` activation.
#[derive(Debug)]
pub struct ActiveScope {
    pub id: ScopeId,
    pub instance_order: Vec<RegistrationId>,
}

impl ActiveScope {
    pub fn new(id: ScopeId) -> Self {
        Self {
            id,
            instance_order: Vec::new(),
        }
    }

    /// Record that `registration` has been instantiated within this scope. Idempotent on a
    /// given registration id so transient lookups never duplicate dispose entries.
    pub fn record_instance(&mut self, registration: RegistrationId) {
        if !self.instance_order.contains(&registration) {
            self.instance_order.push(registration);
        }
    }
}

/// LIFO stack of active scopes. Scope[0] is always the global scope, which is pushed by
/// `RuntimeContainer::launch` and popped by `shutdown`.
#[derive(Debug, Default)]
pub struct ScopeStack {
    frames: Vec<ActiveScope>,
}

impl ScopeStack {
    pub fn push(&mut self, scope: ScopeId) {
        self.frames.push(ActiveScope::new(scope));
    }

    pub fn pop(&mut self) -> Option<ActiveScope> {
        self.frames.pop()
    }

    pub fn top(&self) -> Option<&ActiveScope> {
        self.frames.last()
    }

    pub fn top_mut(&mut self) -> Option<&mut ActiveScope> {
        self.frames.last_mut()
    }

    pub fn frames(&self) -> &[ActiveScope] {
        &self.frames
    }

    pub fn frames_mut(&mut self) -> &mut [ActiveScope] {
        &mut self.frames
    }

    pub fn depth(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Iterate frames from innermost to outermost.
    pub fn iter_innermost(&self) -> impl Iterator<Item = &ActiveScope> {
        self.frames.iter().rev()
    }
}
