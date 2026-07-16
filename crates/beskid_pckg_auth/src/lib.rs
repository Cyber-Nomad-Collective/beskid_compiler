//! Authentication seams for Auth Hub handoffs and pckg API keys.

use std::{
    collections::BTreeSet,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandoffRequest {
    pub app: String,
    pub handoff: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthHubIdentity {
    pub subject: String,
    pub github_login: String,
    pub hub_session_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthHubHandoffClaims {
    pub app: String,
    #[serde(rename = "sub")]
    pub subject: String,
    pub login: String,
    pub sid: String,
    #[serde(rename = "exp")]
    pub expires_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PckgSessionClaims {
    #[serde(rename = "sub")]
    pub subject: String,
    #[serde(rename = "githubLogin")]
    pub github_login: String,
    #[serde(rename = "hubSessionId")]
    pub hub_session_id: String,
    #[serde(rename = "exp")]
    pub expires_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiKeyIdentity {
    pub key_id: String,
    pub subject: String,
    pub scopes: Vec<String>,
}

impl ApiKeyIdentity {
    /// Keeps the persisted/wire-compatible string list while giving route adapters a
    /// typed scope check.
    pub fn has_scope(&self, scope: ApiKeyScope) -> bool {
        self.scopes
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(scope.as_str()))
    }
}

/// The two API-key scopes supported by the legacy registry contract.
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

/// Registry roles assigned to an Auth Hub subject by the pckg persistence adapter.
/// Auth Hub remains responsible for authenticating GitHub identities; pckg owns
/// its local operational roles and therefore resolves these claims separately.
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
    pub fn from_auth_hub(
        identity: AuthHubIdentity,
        roles: impl IntoIterator<Item = SubjectRole>,
    ) -> Self {
        Self::from_subject(identity.subject, roles)
    }

    pub fn from_api_key(identity: ApiKeyIdentity) -> Self {
        Self::from_subject(identity.subject, [SubjectRole::User])
    }

    pub fn from_subject(
        subject: impl Into<String>,
        roles: impl IntoIterator<Item = SubjectRole>,
    ) -> Self {
        Self {
            subject: subject.into(),
            roles: roles.into_iter().collect(),
        }
    }

    pub fn subject(&self) -> &str {
        &self.subject
    }

    pub fn has_role(&self, role: SubjectRole) -> bool {
        self.roles.contains(&role)
    }
}

/// Resource visibility semantics deliberately preserve legacy private-resource
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
        Self {
            subject: subject.into(),
            action,
        }
    }

    fn permits(&self, principal: &Principal, action: ResourceAction) -> bool {
        self.subject == principal.subject && self.action == action
    }
}

/// Typed results for HTTP adapters. Adapters map these directly to 401, 403 and
/// 404 without needing to interpret strings or database-specific errors.
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
/// storage-projected grants. Database and HTTP concerns stay outside this crate.
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
    let is_global_moderator =
        action == ResourceAction::Moderate && principal.has_role(SubjectRole::Moderator);
    let has_explicit_grant = grants
        .into_iter()
        .any(|grant| grant.permits(principal, action));

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
    #[error("Auth Hub handoff is for an unsupported app")]
    UnsupportedApp,
    #[error("authentication credentials were rejected")]
    Rejected,
    #[error("authentication configuration is missing")]
    MissingConfiguration,
    #[error("authentication credentials do not have the required scope")]
    InsufficientScope,
}

/// Verifies the opaque one-time handoff delivered by the central Auth Hub.
///
/// Implementations live at the service boundary so this compatibility core never
/// parses or verifies provider tokens itself.
pub trait AuthHubHandoffVerifier: Send + Sync {
    fn verify(&self, request: HandoffRequest) -> Result<AuthHubIdentity, AuthError>;
}

/// Verifies a pckg-owned automation key. The raw key is never persisted here.
pub trait ApiKeyVerifier: Send + Sync {
    fn verify(&self, raw_key: &str) -> Result<ApiKeyIdentity, AuthError>;

    /// Verifies a raw pckg API key through the injected persistence adapter, then
    /// enforces the operation's typed scope at the boundary.
    fn verify_scoped(
        &self,
        raw_key: &str,
        required_scope: ApiKeyScope,
    ) -> Result<ApiKeyIdentity, AuthError> {
        let identity = self.verify(raw_key)?;
        if identity.has_scope(required_scope) {
            Ok(identity)
        } else {
            Err(AuthError::InsufficientScope)
        }
    }
}

#[derive(Debug, Default)]
pub struct RejectingAuthHubHandoffVerifier;

impl AuthHubHandoffVerifier for RejectingAuthHubHandoffVerifier {
    fn verify(&self, request: HandoffRequest) -> Result<AuthHubIdentity, AuthError> {
        if request.app != "pckg" {
            return Err(AuthError::UnsupportedApp);
        }
        Err(AuthError::Rejected)
    }
}

#[derive(Debug, Clone)]
pub struct Hs256AuthHubHandoffVerifier {
    service_token: String,
}

impl Hs256AuthHubHandoffVerifier {
    pub fn new(service_token: impl Into<String>) -> Result<Self, AuthError> {
        let service_token = service_token.into();
        if service_token.trim().is_empty() {
            return Err(AuthError::MissingConfiguration);
        }
        Ok(Self { service_token })
    }
}

impl AuthHubHandoffVerifier for Hs256AuthHubHandoffVerifier {
    fn verify(&self, request: HandoffRequest) -> Result<AuthHubIdentity, AuthError> {
        if request.app != "pckg" {
            return Err(AuthError::UnsupportedApp);
        }

        let claims = decode::<AuthHubHandoffClaims>(
            &request.handoff,
            &DecodingKey::from_secret(self.service_token.as_bytes()),
            &Validation::new(Algorithm::HS256),
        )
        .map_err(|_| AuthError::Rejected)?
        .claims;

        if claims.app != "pckg"
            || claims.subject.trim().is_empty()
            || claims.login.trim().is_empty()
            || claims.sid.trim().is_empty()
        {
            return Err(AuthError::Rejected);
        }

        Ok(AuthHubIdentity {
            subject: claims.subject,
            github_login: claims.login,
            hub_session_id: claims.sid,
        })
    }
}

pub fn sign_auth_hub_handoff(
    claims: &AuthHubHandoffClaims,
    service_token: &str,
) -> Result<String, AuthError> {
    encode(
        &Header::new(Algorithm::HS256),
        claims,
        &EncodingKey::from_secret(service_token.as_bytes()),
    )
    .map_err(|_| AuthError::Rejected)
}

pub fn issue_pckg_session(
    identity: &AuthHubIdentity,
    session_secret: &str,
) -> Result<String, AuthError> {
    let expires_at = SystemTime::now()
        .checked_add(Duration::from_secs(8 * 60 * 60))
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .ok_or(AuthError::Rejected)?;
    let claims = PckgSessionClaims {
        subject: identity.subject.clone(),
        github_login: identity.github_login.clone(),
        hub_session_id: identity.hub_session_id.clone(),
        expires_at,
    };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(session_secret.as_bytes()),
    )
    .map_err(|_| AuthError::Rejected)
}

pub fn verify_pckg_session(
    session_token: &str,
    session_secret: &str,
) -> Result<AuthHubIdentity, AuthError> {
    let claims = decode::<PckgSessionClaims>(
        session_token,
        &DecodingKey::from_secret(session_secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )
    .map_err(|_| AuthError::Rejected)?
    .claims;
    if claims.subject.trim().is_empty()
        || claims.github_login.trim().is_empty()
        || claims.hub_session_id.trim().is_empty()
    {
        return Err(AuthError::Rejected);
    }
    Ok(AuthHubIdentity {
        subject: claims.subject,
        github_login: claims.github_login,
        hub_session_id: claims.hub_session_id,
    })
}

#[derive(Debug, Default)]
pub struct RejectingApiKeyVerifier;

impl ApiKeyVerifier for RejectingApiKeyVerifier {
    fn verify(&self, _raw_key: &str) -> Result<ApiKeyIdentity, AuthError> {
        Err(AuthError::Rejected)
    }
}
