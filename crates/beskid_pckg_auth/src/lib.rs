//! Authentication seams for the pckg registry.
//!
//! pckg is a resource server that trusts Authelia's forward-auth session.
//! Authelia is the sole OpenID Connect provider; pckg never authenticates
//! users itself. In production (`SHELL_AUTH_MODE=authelia`) the HTTP adapter
//! reads the `Remote-User`, `Remote-Email`, `Remote-Name` and `Remote-Groups`
//! headers injected by Authelia's forward-auth flow. In local development
//! (`SHELL_AUTH_MODE=mock`) the adapter mints a single configurable dev
//! principal so the registry can run without an Authelia instance.
//!
//! pckg-owned API keys remain the only credential the registry issues itself;
//! they are used by CLI tools publishing packages and are verified through a
//! storage-backed adapter, not by anything in this crate.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The runtime authentication mode selected by `SHELL_AUTH_MODE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    /// `SHELL_AUTH_MODE=mock`: a single configurable dev principal is trusted
    /// for every request. Intended for local development only.
    Mock,
    /// `SHELL_AUTH_MODE=authelia`: the request principal is derived from the
    /// `Remote-*` headers injected by Authelia forward-auth.
    Authelia,
}

impl AuthMode {
    pub fn parse(value: &str) -> Result<Self, AuthError> {
        match value.trim() {
            "mock" => Ok(Self::Mock),
            "authelia" => Ok(Self::Authelia),
            _ => Err(AuthError::MissingConfiguration),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mock => "mock",
            Self::Authelia => "authelia",
        }
    }
}

/// The identity Authelia forward-auth projects into a trusted request. The
/// subject is the stable Authelia username (`Remote-User`); it is the only
/// identity key pckg persists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutheliaIdentity {
    pub subject: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub groups: Vec<String>,
}

/// pckg-owned automation key identity. The raw key is never persisted by the
/// registry; only its SHA-256 digest is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiKeyIdentity {
    pub key_id: String,
    pub subject: String,
    pub scopes: Vec<String>,
}

impl ApiKeyIdentity {
    /// Keeps the persisted/wire-compatible string list while giving route
    /// adapters a typed scope check.
    pub fn has_scope(&self, scope: ApiKeyScope) -> bool {
        self.scopes.iter().any(|candidate| candidate.eq_ignore_ascii_case(scope.as_str()))
    }
}

/// The two API-key scopes supported by the registry contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ApiKeyScope {
    Read,
    Publish,
}

impl ApiKeyScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Publish => "publish",
        }
    }
}

/// Registry roles assigned to a subject. In production these are projected
/// from Authelia groups (`Remote-Groups`); pckg never grants them itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SubjectRole {
    User,
    Moderator,
    SuperAdmin,
}

/// Authenticated request identity plus pckg-local role claims.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Principal {
    subject: String,
    roles: BTreeSet<SubjectRole>,
}

impl Principal {
    pub fn from_subject(subject: impl Into<String>, roles: impl IntoIterator<Item = SubjectRole>) -> Self {
        Self { subject: subject.into(), roles: roles.into_iter().collect() }
    }

    /// Builds a principal from an Authelia identity, mapping the configured
    /// admin/moderator groups to pckg roles. Every authenticated subject is at
    /// least a `User`; group membership only adds elevated roles.
    pub fn from_authelia(identity: &AutheliaIdentity, admin_group: &str, moderator_group: &str) -> Self {
        let mut roles = BTreeSet::new();
        roles.insert(SubjectRole::User);
        for group in &identity.groups {
            if group == admin_group {
                roles.insert(SubjectRole::SuperAdmin);
            } else if group == moderator_group {
                roles.insert(SubjectRole::Moderator);
            }
        }
        Self::from_subject(identity.subject.clone(), roles)
    }

    pub fn from_api_key(identity: ApiKeyIdentity) -> Self {
        Self::from_subject(identity.subject, [SubjectRole::User])
    }

    pub fn subject(&self) -> &str {
        &self.subject
    }

    pub fn roles(&self) -> &BTreeSet<SubjectRole> {
        &self.roles
    }

    pub fn has_role(&self, role: SubjectRole) -> bool {
        self.roles.contains(&role)
    }
}

/// Resource visibility semantics deliberately preserve private-resource
/// concealment: callers without access get a typed `NotFound`, not a leak.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceVisibility {
    Public,
    Private,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceAction {
    Read,
    Publish,
    Moderate,
    Manage,
}

/// A storage adapter projects a persisted resource permission into this value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionGrant {
    subject: String,
    action: ResourceAction,
}

impl PermissionGrant {
    pub fn new(subject: impl Into<String>, action: ResourceAction) -> Self {
        Self { subject: subject.into(), action }
    }

    fn permits(&self, principal: &Principal, action: ResourceAction) -> bool {
        self.subject == principal.subject && self.action == action
    }
}

/// Typed results for HTTP adapters. Adapters map these directly to 401, 403
/// and 404 without needing to interpret strings or database-specific errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum AuthorizationError {
    #[error("authentication required")]
    Unauthorized,
    #[error("resource action is not permitted")]
    Forbidden,
    #[error("resource not found")]
    NotFound,
}

/// Decides resource access using only subject, role claims, ownership, and
/// storage-projected grants. Database and HTTP concerns stay outside this
/// crate.
pub fn authorize_resource_access(
    principal: Option<&Principal>,
    owner_subject: &str,
    visibility: ResourceVisibility,
    action: ResourceAction,
    grants: impl IntoIterator<Item = PermissionGrant>,
) -> Result<(), AuthorizationError> {
    if action == ResourceAction::Read && visibility == ResourceVisibility::Public {
        return Ok(());
    }

    let Some(principal) = principal else {
        return match action {
            ResourceAction::Read => Err(AuthorizationError::NotFound),
            ResourceAction::Publish | ResourceAction::Moderate | ResourceAction::Manage => {
                Err(AuthorizationError::Unauthorized)
            }
        };
    };

    let is_owner = principal.subject == owner_subject;
    let is_super_admin = principal.has_role(SubjectRole::SuperAdmin);
    let is_global_moderator = action == ResourceAction::Moderate && principal.has_role(SubjectRole::Moderator);
    let has_explicit_grant = grants.into_iter().any(|grant| grant.permits(principal, action));

    if is_owner || is_super_admin || is_global_moderator || has_explicit_grant {
        return Ok(());
    }

    match action {
        ResourceAction::Read => Err(AuthorizationError::NotFound),
        ResourceAction::Publish | ResourceAction::Moderate | ResourceAction::Manage => {
            Err(AuthorizationError::Forbidden)
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AuthError {
    #[error("authentication configuration is missing")]
    MissingConfiguration,
    #[error("authentication credentials were rejected")]
    Rejected,
    #[error("authentication credentials do not have the required scope")]
    InsufficientScope,
}

/// Verifies a pckg-owned automation key. The raw key is never persisted here.
pub trait ApiKeyVerifier: Send + Sync {
    fn verify(&self, raw_key: &str) -> Result<ApiKeyIdentity, AuthError>;

    /// Verifies a raw pckg API key through the injected persistence adapter,
    /// then enforces the operation's typed scope at the boundary.
    fn verify_scoped(&self, raw_key: &str, required_scope: ApiKeyScope) -> Result<ApiKeyIdentity, AuthError> {
        let identity = self.verify(raw_key)?;
        if identity.has_scope(required_scope) { Ok(identity) } else { Err(AuthError::InsufficientScope) }
    }
}

#[derive(Debug, Default)]
pub struct RejectingApiKeyVerifier;

impl ApiKeyVerifier for RejectingApiKeyVerifier {
    fn verify(&self, _raw_key: &str) -> Result<ApiKeyIdentity, AuthError> {
        Err(AuthError::Rejected)
    }
}

/// Wire shape for the `/api/auth/session` response. The frontend reads the
/// Authelia-projected subject, optional contact metadata, and group
/// membership so it can render role-gated UI without a second round-trip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionIdentity {
    pub subject: String,
    pub email: Option<String>,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    pub groups: Vec<String>,
}

/// Validates that a subject is a non-empty, trimmed identifier suitable for
/// persistence. Authelia usernames, `github:<numeric-id>` subjects, and any
/// comparable opaque identifier are accepted; whitespace, control
/// characters, and empty values are rejected.
pub fn is_valid_subject(subject: &str) -> bool {
    let trimmed = subject.trim();
    !trimmed.is_empty()
        && trimmed == subject
        && trimmed.bytes().all(|byte| byte.is_ascii_graphic())
        && trimmed.len() <= 255
}
