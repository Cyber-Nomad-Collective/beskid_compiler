use std::{
    collections::BTreeSet,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

use axum::response::Response;
use beskid_pckg_community::{CommunityService, Subject};
use beskid_pckg_store::{
    AdminRole, AsyncAdministrationRepository, AsyncCommunityRepository, CommunityStoreError, SqlxCommunityRepository,
    SqlxPackageRepository,
};

use super::error::unavailable;

#[derive(Clone)]
pub struct CommunityState {
    pub(super) session_secret: Option<String>,
    pub(super) backend: CommunityBackend,
    pub(super) moderation: ModerationBackend,
    pub(super) policy: Option<Arc<dyn CommunityLinkPolicy>>,
}

/// The deliberately small profile projection exposed to registry-owned
/// catalog routes.  It contains only persisted community profile fields; in
/// particular it never invents a GitHub login from an Auth Hub subject.
#[derive(Clone, Debug)]
#[allow(dead_code)] // Used by packages.rs; integration test path-includes this module alone.
pub(crate) struct CatalogProfile {
    pub subject: String,
    pub display_name: String,
    pub bio: String,
    pub social_links: Vec<String>,
    pub is_publisher_verified: bool,
}

#[derive(Clone)]
pub(super) enum CommunityBackend {
    InMemory(Arc<Mutex<CommunityService>>),
    #[allow(dead_code)] // Constructed via with_sqlx_session_secret in lib.rs.
    Sqlx(Arc<SqlxCommunityRepository>),
}

#[derive(Clone)]
pub(super) enum ModerationBackend {
    InMemory(Arc<Mutex<BTreeSet<String>>>),
    #[allow(dead_code)] // Constructed via with_sqlx_session_secret in lib.rs.
    Sqlx(Arc<SqlxPackageRepository>),
}

pub(crate) type CommunityLinkPolicyFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Option<&'static str>, ()>> + Send + 'a>>;

pub(crate) trait CommunityLinkPolicy: Send + Sync {
    fn block_reason<'a>(&'a self, text: &'a str) -> CommunityLinkPolicyFuture<'a>;
}

impl Default for CommunityState {
    fn default() -> Self {
        Self {
            session_secret: None,
            backend: CommunityBackend::InMemory(Arc::new(Mutex::new(CommunityService::new()))),
            moderation: ModerationBackend::InMemory(Arc::new(Mutex::new(BTreeSet::new()))),
            policy: None,
        }
    }
}

impl CommunityState {
    pub fn with_session_secret(session_secret: impl Into<String>) -> Self {
        Self {
            session_secret: Some(session_secret.into()),
            backend: CommunityBackend::InMemory(Arc::new(Mutex::new(CommunityService::new()))),
            moderation: ModerationBackend::InMemory(Arc::new(Mutex::new(BTreeSet::new()))),
            policy: None,
        }
    }

    #[allow(dead_code)] // Used by AppState construction in lib.rs.
    pub fn with_sqlx_session_secret(
        session_secret: impl Into<String>,
        repository: Arc<SqlxCommunityRepository>,
        moderation_repository: Arc<SqlxPackageRepository>,
        policy: Arc<dyn CommunityLinkPolicy>,
    ) -> Self {
        Self {
            session_secret: Some(session_secret.into()),
            backend: CommunityBackend::Sqlx(repository),
            moderation: ModerationBackend::Sqlx(moderation_repository),
            policy: Some(policy),
        }
    }

    #[cfg(test)]
    #[allow(dead_code)] // Seeded by path-included integration tests, not lib unit tests.
    pub fn grant_test_moderator(&self, subject: impl Into<String>) {
        match &self.moderation {
            ModerationBackend::InMemory(subjects) => {
                subjects.lock().expect("moderation service lock is not poisoned").insert(subject.into());
            }
            ModerationBackend::Sqlx(_) => panic!("SQL-backed moderation cannot be seeded directly"),
        }
    }

    pub(super) async fn can_moderate_board(&self, subject: &str, board_id: &str) -> bool {
        match &self.moderation {
            ModerationBackend::InMemory(subjects) => {
                subjects.lock().expect("moderation service lock is not poisoned").contains(subject)
            }
            ModerationBackend::Sqlx(repository) => {
                if repository.roles_for_subject(subject).await.is_ok_and(|roles| {
                    roles.iter().any(|role| matches!(role, AdminRole::Moderator | AdminRole::SuperAdmin))
                }) {
                    return true;
                }
                repository.list_resource_permissions("board", board_id).await.is_ok_and(|grants| {
                    grants.iter().any(|grant| grant.subject == subject && grant.capability == "moderate")
                })
            }
        }
    }

    #[allow(dead_code)] // Used by the direct HTTP adapter tests to seed an in-memory board.
    #[allow(dead_code)] // Integration tests use this controlled board-seeding seam.
    pub fn service(&self) -> &Arc<Mutex<CommunityService>> {
        match &self.backend {
            CommunityBackend::InMemory(service) => service,
            CommunityBackend::Sqlx(_) => {
                panic!("SQL-backed community state does not expose an in-memory service")
            }
        }
    }

    #[allow(dead_code)] // Used by packages.rs catalog projection.
    pub(crate) async fn profile_for_catalog(
        &self,
        subject: &str,
    ) -> Result<Option<CatalogProfile>, CommunityStoreError> {
        let Ok(subject) = Subject::new(subject.to_owned()) else {
            return Ok(None);
        };
        match &self.backend {
            CommunityBackend::InMemory(service) => {
                Ok(service.lock().expect("community service lock is not poisoned").profile(&subject).map(|profile| {
                    CatalogProfile {
                        subject: profile.subject.as_str().to_owned(),
                        display_name: profile.display_name.clone(),
                        bio: profile.bio.clone(),
                        social_links: profile.social_links.clone(),
                        is_publisher_verified: profile.is_publisher_verified,
                    }
                }))
            }
            CommunityBackend::Sqlx(repository) => repository.profile(subject.as_str()).await.map(|profile| {
                profile.map(|profile| CatalogProfile {
                    subject: profile.subject,
                    display_name: profile.display_name,
                    bio: profile.bio,
                    social_links: serde_json::from_str(&profile.social_links_json).unwrap_or_default(),
                    is_publisher_verified: profile.is_publisher_verified,
                })
            }),
        }
    }

    pub(super) async fn blocked_link_reason(&self, text: &str) -> Result<Option<&'static str>, Response> {
        match &self.policy {
            Some(policy) => policy.block_reason(text).await.map_err(|_| unavailable()),
            None => Ok(None),
        }
    }
}

pub(super) fn now_unix_seconds() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |duration| duration.as_secs() as i64)
}
