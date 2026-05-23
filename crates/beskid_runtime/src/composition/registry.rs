//! Stable identifiers and metadata for native dependency-injection registrations.
//!
//! The runtime container consumes the `BindingPlan` produced by
//! `beskid_analysis::composition` (registration ordering, scope tree, plural bindings).
//! Codegen passes the same identifiers through the C ABI when wiring host launch and
//! `with` scopes, so the in-memory `RegistrationId` and `ScopeId` values must round-trip
//! losslessly with the integer encoding used over `extern "C-unwind"` boundaries.

use std::fmt;

/// Opaque id for a service registration emitted by composition analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RegistrationId(pub u32);

impl RegistrationId {
    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl fmt::Display for RegistrationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Registration#{}", self.0)
    }
}

/// Opaque id for a composition scope (global == 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ScopeId(pub u32);

impl ScopeId {
    pub const GLOBAL: Self = Self(0);

    pub const fn raw(self) -> u32 {
        self.0
    }

    pub const fn is_global(self) -> bool {
        self.0 == 0
    }
}

impl fmt::Display for ScopeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Scope#{}", self.0)
    }
}

/// Lifetime of a service registration; mirrors `beskid_analysis::composition::RegistrationLifetime`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifetime {
    /// One instance per `with` scope activation.
    Scoped,
    /// One instance per container launch (global).
    Single,
    /// New instance per resolve.
    Transient,
}

impl Lifetime {
    pub const ABI_SCOPED: i32 = 0;
    pub const ABI_SINGLE: i32 = 1;
    pub const ABI_TRANSIENT: i32 = 2;

    pub const fn to_abi(self) -> i32 {
        match self {
            Lifetime::Scoped => Self::ABI_SCOPED,
            Lifetime::Single => Self::ABI_SINGLE,
            Lifetime::Transient => Self::ABI_TRANSIENT,
        }
    }

    pub const fn from_abi(value: i32) -> Option<Self> {
        match value {
            Self::ABI_SCOPED => Some(Lifetime::Scoped),
            Self::ABI_SINGLE => Some(Lifetime::Single),
            Self::ABI_TRANSIENT => Some(Lifetime::Transient),
            _ => None,
        }
    }
}
