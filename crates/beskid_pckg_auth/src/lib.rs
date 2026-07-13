//! Authentication seams for Auth Hub handoffs and pckg API keys.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AuthError {
    #[error("Auth Hub handoff is for an unsupported app")]
    UnsupportedApp,
    #[error("authentication credentials were rejected")]
    Rejected,
    #[error("authentication configuration is missing")]
    MissingConfiguration,
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
