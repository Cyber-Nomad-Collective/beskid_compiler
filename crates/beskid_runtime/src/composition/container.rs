//! Native dependency-injection container.
//!
//! Implements the runtime contract behind the language-meta `host` / `registry` / `scope`
//! / `with` / `launch` surface. Mirrors the [`BindingPlan`] computed by
//! `beskid_analysis::composition`: scope tree, registration ordering, plural bindings.
//!
//! Phase A constraint: the container runs on the single mutator that owns the runtime
//! scope. All mutation is `&mut self`; concurrent access is the caller's responsibility.

use std::collections::HashMap;

use super::registry::{Lifetime, RegistrationId, ScopeId};
use super::scope::ScopeStack;

/// Type-erased handle for a service instance produced by an [`InstanceFactory`].
pub type InstancePtr = *mut std::ffi::c_void;

/// Closure that materializes a service instance.
pub type InstanceFactory = Box<dyn FnMut(&mut RuntimeContainer) -> InstancePtr>;

/// Closure run after an instance is created, before user code sees it (`init` lifecycle).
pub type InitHook = Box<dyn FnMut(InstancePtr)>;

/// Closure run when the owning scope leaves (`dispose` lifecycle).
pub type DisposeHook = Box<dyn FnMut(InstancePtr)>;

/// One service registration as it appears in the runtime container.
pub struct RegistrationRecord {
    pub id: RegistrationId,
    pub scope: ScopeId,
    pub lifetime: Lifetime,
    pub factory: Option<InstanceFactory>,
    pub init: Option<InitHook>,
    pub dispose: Option<DisposeHook>,
}

impl std::fmt::Debug for RegistrationRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegistrationRecord")
            .field("id", &self.id)
            .field("scope", &self.scope)
            .field("lifetime", &self.lifetime)
            .field("has_factory", &self.factory.is_some())
            .field("has_init", &self.init.is_some())
            .field("has_dispose", &self.dispose.is_some())
            .finish()
    }
}

/// Errors surfaced from the runtime container API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerError {
    /// Asked to leave a scope that does not match the top of the active stack.
    ScopeMismatch { expected: ScopeId, actual: ScopeId },
    /// Tried to leave with an empty scope stack (no matching `with` entered).
    NoActiveScope,
    /// Tried to enter a scope before `launch` pushed the global frame.
    NotLaunched,
    /// Requested an unknown registration id.
    UnknownRegistration(RegistrationId),
    /// Container already running when `launch` was called again.
    AlreadyLaunched,
    /// Tried to resolve before the global scope was activated by `launch`.
    NotActive,
}

impl ContainerError {
    pub const ABI_SCOPE_MISMATCH: i32 = 1;
    pub const ABI_NO_ACTIVE_SCOPE: i32 = 2;
    pub const ABI_NOT_LAUNCHED: i32 = 3;
    pub const ABI_UNKNOWN_REGISTRATION: i32 = 4;
    pub const ABI_ALREADY_LAUNCHED: i32 = 5;
    pub const ABI_NOT_ACTIVE: i32 = 6;

    pub const fn to_abi(&self) -> i32 {
        match self {
            ContainerError::ScopeMismatch { .. } => Self::ABI_SCOPE_MISMATCH,
            ContainerError::NoActiveScope => Self::ABI_NO_ACTIVE_SCOPE,
            ContainerError::NotLaunched => Self::ABI_NOT_LAUNCHED,
            ContainerError::UnknownRegistration(_) => Self::ABI_UNKNOWN_REGISTRATION,
            ContainerError::AlreadyLaunched => Self::ABI_ALREADY_LAUNCHED,
            ContainerError::NotActive => Self::ABI_NOT_ACTIVE,
        }
    }
}

impl std::fmt::Display for ContainerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContainerError::ScopeMismatch { expected, actual } => write!(
                f,
                "composition: scope_leave({expected}) did not match active scope {actual}"
            ),
            ContainerError::NoActiveScope => {
                f.write_str("composition: scope_leave called with empty stack")
            }
            ContainerError::NotLaunched => {
                f.write_str("composition: scope_enter called before launch")
            }
            ContainerError::UnknownRegistration(id) => {
                write!(f, "composition: unknown registration {id}")
            }
            ContainerError::AlreadyLaunched => {
                f.write_str("composition: launch called on an already-active container")
            }
            ContainerError::NotActive => {
                f.write_str("composition: container is not active (call launch first)")
            }
        }
    }
}

impl std::error::Error for ContainerError {}

/// Runtime container that powers the language-meta DI surface.
///
/// Construction is cheap (`RuntimeContainer::new`); call [`register`](Self::register) for each
/// emitted [`Registration`], [`bind_plural`](Self::bind_plural) for each `T[]` inject site,
/// then [`launch`](Self::launch) to push the global scope and start handing out instances.
pub struct RuntimeContainer {
    registrations: HashMap<RegistrationId, RegistrationRecord>,
    registration_order: Vec<RegistrationId>,
    plural: HashMap<RegistrationId, Vec<RegistrationId>>,
    singletons: HashMap<RegistrationId, InstancePtr>,
    scoped_instances: HashMap<(u32, RegistrationId), InstancePtr>,
    scope_activation_counter: u32,
    active_scope_keys: Vec<u32>,
    stack: ScopeStack,
    launched: bool,
}

impl Default for RuntimeContainer {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeContainer {
    pub fn new() -> Self {
        Self {
            registrations: HashMap::new(),
            registration_order: Vec::new(),
            plural: HashMap::new(),
            singletons: HashMap::new(),
            scoped_instances: HashMap::new(),
            scope_activation_counter: 0,
            active_scope_keys: Vec::new(),
            stack: ScopeStack::default(),
            launched: false,
        }
    }

    /// Register a service. Order of `register` calls becomes the
    /// `registration_order` consumed by [`launch`](Self::launch) when initializing eager
    /// singletons and by scope disposal (reverse order).
    pub fn register(&mut self, record: RegistrationRecord) {
        let id = record.id;
        if self.registrations.insert(id, record).is_none() {
            self.registration_order.push(id);
        }
    }

    /// Bind the plural inject site rooted at `owner` to the listed registrations. The order
    /// of `targets` is preserved when [`resolve_plural`](Self::resolve_plural) is called.
    pub fn bind_plural(&mut self, owner: RegistrationId, targets: Vec<RegistrationId>) {
        self.plural.insert(owner, targets);
    }

    /// Push the global scope and eagerly create any singletons whose factory is registered.
    /// Returns the singleton instances created, in registration order.
    pub fn launch(&mut self) -> Result<Vec<InstancePtr>, ContainerError> {
        if self.launched {
            return Err(ContainerError::AlreadyLaunched);
        }
        self.launched = true;
        self.push_scope_frame(ScopeId::GLOBAL);

        let mut singletons = Vec::new();
        let order = self.registration_order.clone();
        for id in order {
            let needs_eager = match self.registrations.get(&id) {
                Some(r) => r.scope.is_global() && matches!(r.lifetime, Lifetime::Single),
                None => continue,
            };
            if !needs_eager {
                continue;
            }
            if self
                .registrations
                .get(&id)
                .and_then(|r| r.factory.as_ref())
                .is_some()
            {
                let ptr = self.create_instance(id)?;
                singletons.push(ptr);
            }
        }
        Ok(singletons)
    }

    /// Tear down the launched container: pops the global scope, running every dispose hook
    /// in reverse registration order.
    pub fn shutdown(&mut self) -> Result<(), ContainerError> {
        if !self.launched {
            return Err(ContainerError::NotActive);
        }
        while !self.stack.is_empty() {
            let scope = self.stack.top().expect("non-empty stack").id;
            self.leave_scope(scope)?;
        }
        self.launched = false;
        Ok(())
    }

    /// Push a new active scope frame (used by `with <scope> { ... }` lowering).
    pub fn enter_scope(&mut self, scope: ScopeId) -> Result<(), ContainerError> {
        if !self.launched {
            return Err(ContainerError::NotLaunched);
        }
        self.push_scope_frame(scope);
        Ok(())
    }

    /// Pop the active scope frame, running each registered `dispose` hook in reverse
    /// registration order for instances created inside the scope.
    pub fn leave_scope(&mut self, expected: ScopeId) -> Result<(), ContainerError> {
        let frame = self.stack.pop().ok_or(ContainerError::NoActiveScope)?;
        if frame.id != expected {
            // restore + report mismatch
            self.stack.push(frame.id);
            return Err(ContainerError::ScopeMismatch {
                expected,
                actual: frame.id,
            });
        }
        let scope_key = self
            .active_scope_keys
            .pop()
            .expect("scope key stack parallel to scope stack");
        // dispose in reverse registration order
        for id in frame.instance_order.into_iter().rev() {
            let instance_key = (scope_key, id);
            let ptr = self.scoped_instances.remove(&instance_key);
            if let Some(ptr) = ptr
                && let Some(record) = self.registrations.get_mut(&id)
                && let Some(dispose) = record.dispose.as_mut()
            {
                dispose(ptr);
            }
            // For singleton-scoped (Lifetime::Single in global), also drop from singletons
            // when the global scope is leaving.
            if expected.is_global()
                && let Some(ptr) = self.singletons.remove(&id)
                && let Some(record) = self.registrations.get_mut(&id)
                && let Some(dispose) = record.dispose.as_mut()
            {
                dispose(ptr);
            }
        }
        Ok(())
    }

    /// Resolve a single instance for `id` honoring its lifetime and the currently active
    /// scope stack.
    pub fn resolve(&mut self, id: RegistrationId) -> Result<InstancePtr, ContainerError> {
        if !self.launched {
            return Err(ContainerError::NotActive);
        }
        if !self.registrations.contains_key(&id) {
            return Err(ContainerError::UnknownRegistration(id));
        }
        self.create_instance(id)
    }

    /// Resolve a plural inject site for `owner`. Returns the bound targets in their original
    /// registration order; each target is resolved with the same lifetime rules as
    /// [`resolve`](Self::resolve).
    pub fn resolve_plural(
        &mut self,
        owner: RegistrationId,
    ) -> Result<Vec<InstancePtr>, ContainerError> {
        if !self.launched {
            return Err(ContainerError::NotActive);
        }
        let targets = self.plural.get(&owner).cloned().unwrap_or_default();
        let mut out = Vec::with_capacity(targets.len());
        for target in targets {
            out.push(self.resolve(target)?);
        }
        Ok(out)
    }

    /// Read the depth of the active scope stack (1 means only the global scope is active).
    pub fn scope_depth(&self) -> usize {
        self.stack.depth()
    }

    /// Number of registered services.
    pub fn registration_count(&self) -> usize {
        self.registrations.len()
    }

    /// Number of plural-inject sites bound.
    pub fn plural_binding_count(&self) -> usize {
        self.plural.len()
    }

    fn push_scope_frame(&mut self, scope: ScopeId) {
        self.scope_activation_counter = self.scope_activation_counter.wrapping_add(1);
        self.active_scope_keys.push(self.scope_activation_counter);
        self.stack.push(scope);
    }

    fn create_instance(&mut self, id: RegistrationId) -> Result<InstancePtr, ContainerError> {
        let (lifetime, _scope) = {
            let record = self
                .registrations
                .get(&id)
                .ok_or(ContainerError::UnknownRegistration(id))?;
            (record.lifetime, record.scope)
        };

        match lifetime {
            Lifetime::Single => {
                if let Some(ptr) = self.singletons.get(&id) {
                    return Ok(*ptr);
                }
                let ptr = self.invoke_factory(id);
                self.singletons.insert(id, ptr);
                self.run_init(id, ptr);
                // Singletons dispose at shutdown via the global frame.
                if let Some(frame) = self.stack.frames_mut().first_mut() {
                    frame.record_instance(id);
                }
                Ok(ptr)
            }
            Lifetime::Scoped => {
                let frame_idx = self
                    .stack
                    .depth()
                    .checked_sub(1)
                    .ok_or(ContainerError::NoActiveScope)?;
                let scope_key = self.active_scope_keys[frame_idx];
                let key = (scope_key, id);
                if let Some(ptr) = self.scoped_instances.get(&key) {
                    return Ok(*ptr);
                }
                let ptr = self.invoke_factory(id);
                self.scoped_instances.insert(key, ptr);
                self.run_init(id, ptr);
                if let Some(frame) = self.stack.frames_mut().get_mut(frame_idx) {
                    frame.record_instance(id);
                }
                Ok(ptr)
            }
            Lifetime::Transient => {
                let ptr = self.invoke_factory(id);
                self.run_init(id, ptr);
                // Transient instances are not tracked for dispose by the container.
                Ok(ptr)
            }
        }
    }

    fn invoke_factory(&mut self, id: RegistrationId) -> InstancePtr {
        // Temporarily take the factory out so we can pass `&mut self` to the closure.
        let mut taken = self
            .registrations
            .get_mut(&id)
            .and_then(|r| r.factory.take());
        let ptr = if let Some(factory) = taken.as_mut() {
            factory(self)
        } else {
            std::ptr::null_mut()
        };
        if let Some(record) = self.registrations.get_mut(&id) {
            record.factory = taken;
        }
        ptr
    }

    fn run_init(&mut self, id: RegistrationId, ptr: InstancePtr) {
        let mut taken = self.registrations.get_mut(&id).and_then(|r| r.init.take());
        if let Some(init) = taken.as_mut() {
            init(ptr);
        }
        if let Some(record) = self.registrations.get_mut(&id) {
            record.init = taken;
        }
    }
}
