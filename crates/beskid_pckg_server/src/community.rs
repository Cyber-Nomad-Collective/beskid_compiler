//! NodeBB community integration.
//!
//! Every public package owns one discussion category under a configured
//! "Packages" parent category on the NodeBB community site. The registry
//! persists only the category identity (`cid` + slug); topics, posts, and
//! moderation stay in NodeBB. The link is created when a public package is
//! created and can be repaired by an administrator through
//! `POST /api/admin/community/reconcile`. Package creation never fails because
//! NodeBB is unavailable: the failure is recorded in the registry activity log
//! and the package page simply shows no discussion link until the link exists.
//!
//! Configuration is all-or-nothing (`PCKG_NODEBB_API_URL`,
//! `PCKG_NODEBB_ADMIN_TOKEN`, `PCKG_NODEBB_PACKAGES_PARENT_CID`,
//! `PCKG_COMMUNITY_URL`); a partial set is a startup error so a deployment can
//! never half-configure the integration and silently publish dead links.

use std::{env, sync::Arc, time::Duration};

use async_trait::async_trait;
use beskid_pckg_store::{NewRegistryActivity, Package, PackageCommunity, StoreError};
use serde::Deserialize;

use crate::server::ServerStartupError;

/// Validated NodeBB settings taken from the process environment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommunityConfig {
    /// Internal NodeBB origin used for Write API calls (for example
    /// `http://community:4567`).
    pub api_url: String,
    /// Public NodeBB origin used in links shown to users.
    pub public_url: String,
    /// NodeBB Write API bearer token with category administration rights.
    pub admin_token: String,
    /// Parent category every package category is created under.
    pub packages_parent_cid: i64,
}

impl CommunityConfig {
    const VARIABLES: [&'static str; 4] =
        ["PCKG_NODEBB_API_URL", "PCKG_NODEBB_ADMIN_TOKEN", "PCKG_NODEBB_PACKAGES_PARENT_CID", "PCKG_COMMUNITY_URL"];

    /// Reads the integration settings. Returns `Ok(None)` when none of the
    /// variables is set and an error when only some of them are.
    pub fn from_environment() -> Result<Option<Self>, ServerStartupError> {
        let values = Self::VARIABLES
            .iter()
            .map(|name| env::var(name).ok().map(|value| value.trim().to_owned()).filter(|value| !value.is_empty()))
            .collect::<Vec<_>>();
        if values.iter().all(Option::is_none) {
            return Ok(None);
        }
        let missing = Self::VARIABLES
            .iter()
            .zip(&values)
            .filter(|(_, value)| value.is_none())
            .map(|(name, _)| *name)
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(ServerStartupError(format!(
                "NodeBB community integration is partially configured; missing {}",
                missing.join(", ")
            )));
        }
        let [api_url, admin_token, parent_cid, public_url] =
            [values[0].clone(), values[1].clone(), values[2].clone(), values[3].clone()].map(Option::unwrap);
        let packages_parent_cid = parent_cid.parse::<i64>().ok().filter(|cid| *cid > 0).ok_or_else(|| {
            ServerStartupError("PCKG_NODEBB_PACKAGES_PARENT_CID must be a positive NodeBB category id".to_owned())
        })?;
        for (name, value) in [("PCKG_NODEBB_API_URL", &api_url), ("PCKG_COMMUNITY_URL", &public_url)] {
            if !(value.starts_with("http://") || value.starts_with("https://")) {
                return Err(ServerStartupError(format!("{name} must be an absolute http(s) origin")));
            }
        }
        Ok(Some(Self {
            api_url: api_url.trim_end_matches('/').to_owned(),
            public_url: public_url.trim_end_matches('/').to_owned(),
            admin_token,
            packages_parent_cid,
        }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommunityError {
    /// NodeBB rejected or failed the request; the message is operator-facing.
    Nodebb(String),
    /// NodeBB answered but the payload lacked a usable category identity.
    InvalidResponse(String),
    /// The registry could not persist the link.
    Store(StoreError),
}

impl std::fmt::Display for CommunityError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Nodebb(message) => write!(formatter, "NodeBB request failed: {message}"),
            Self::InvalidResponse(message) => write!(formatter, "NodeBB response is invalid: {message}"),
            Self::Store(error) => write!(formatter, "registry could not persist the community link: {error:?}"),
        }
    }
}

/// Category creation request sent to NodeBB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateCategoryRequest {
    pub name: String,
    pub description: String,
    pub parent_cid: i64,
}

/// Category identity returned by NodeBB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodebbCategory {
    pub cid: i64,
    /// NodeBB slug including the numeric prefix, for example `12/acme-math`.
    pub slug: String,
}

/// Public activity counters of a NodeBB category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NodebbCategoryStats {
    pub topic_count: u64,
    pub post_count: u64,
}

/// Transport seam over the NodeBB Write/Read APIs. Tests inject a double.
#[async_trait]
pub trait NodebbClient: Send + Sync {
    async fn create_category(&self, request: CreateCategoryRequest) -> Result<NodebbCategory, CommunityError>;
    async fn category_stats(&self, cid: i64) -> Result<NodebbCategoryStats, CommunityError>;
}

/// HTTP client over NodeBB's Write API v3 (`/api/v3/categories`) and Read API
/// (`/api/category/{cid}`).
pub struct HttpNodebbClient {
    http: reqwest::Client,
    api_url: String,
    admin_token: String,
}

impl HttpNodebbClient {
    pub fn new(config: &CommunityConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .user_agent("beskid-pckg-server")
            .build()
            .expect("reqwest client builds with static configuration");
        Self { http, api_url: config.api_url.clone(), admin_token: config.admin_token.clone() }
    }
}

#[derive(Deserialize)]
struct WriteApiEnvelope<T> {
    status: WriteApiStatus,
    response: Option<T>,
}

#[derive(Deserialize)]
struct WriteApiStatus {
    code: String,
    #[serde(default)]
    message: String,
}

#[derive(Deserialize)]
struct CategoryPayload {
    cid: Option<serde_json::Value>,
    slug: Option<String>,
    #[serde(default)]
    topic_count: Option<serde_json::Value>,
    #[serde(default)]
    post_count: Option<serde_json::Value>,
}

fn number(value: &Option<serde_json::Value>) -> Option<i64> {
    match value {
        Some(serde_json::Value::Number(number)) => number.as_i64(),
        Some(serde_json::Value::String(text)) => text.parse().ok(),
        _ => None,
    }
}

#[async_trait]
impl NodebbClient for HttpNodebbClient {
    async fn create_category(&self, request: CreateCategoryRequest) -> Result<NodebbCategory, CommunityError> {
        let response = self
            .http
            .post(format!("{}/api/v3/categories", self.api_url))
            .bearer_auth(&self.admin_token)
            .json(&serde_json::json!({
                "name": request.name,
                "description": request.description,
                "parentCid": request.parent_cid,
            }))
            .send()
            .await
            .map_err(|error| CommunityError::Nodebb(error.to_string()))?;
        let status = response.status();
        let envelope: WriteApiEnvelope<CategoryPayload> =
            response.json().await.map_err(|error| CommunityError::InvalidResponse(error.to_string()))?;
        if !status.is_success() || envelope.status.code != "ok" {
            return Err(CommunityError::Nodebb(format!(
                "category creation returned {status}: {} {}",
                envelope.status.code, envelope.status.message
            )));
        }
        let payload = envelope.response.ok_or_else(|| CommunityError::InvalidResponse("missing response".into()))?;
        let cid = number(&payload.cid)
            .filter(|cid| *cid > 0)
            .ok_or_else(|| CommunityError::InvalidResponse("missing category id".into()))?;
        let slug = payload
            .slug
            .filter(|slug| !slug.trim().is_empty())
            .ok_or_else(|| CommunityError::InvalidResponse("missing category slug".into()))?;
        Ok(NodebbCategory { cid, slug })
    }

    async fn category_stats(&self, cid: i64) -> Result<NodebbCategoryStats, CommunityError> {
        let response = self
            .http
            .get(format!("{}/api/category/{cid}", self.api_url))
            .send()
            .await
            .map_err(|error| CommunityError::Nodebb(error.to_string()))?;
        if !response.status().is_success() {
            return Err(CommunityError::Nodebb(format!("category read returned {}", response.status())));
        }
        let payload: CategoryPayload =
            response.json().await.map_err(|error| CommunityError::InvalidResponse(error.to_string()))?;
        Ok(NodebbCategoryStats {
            topic_count: number(&payload.topic_count).unwrap_or_default().max(0) as u64,
            post_count: number(&payload.post_count).unwrap_or_default().max(0) as u64,
        })
    }
}

/// Registry-side behavior over a [`NodebbClient`].
pub(crate) struct CommunityService {
    client: Arc<dyn NodebbClient>,
    public_url: String,
    packages_parent_cid: i64,
}

impl CommunityService {
    pub(crate) fn new(client: Arc<dyn NodebbClient>, public_url: &str, packages_parent_cid: i64) -> Self {
        Self { client, public_url: public_url.trim_end_matches('/').to_owned(), packages_parent_cid }
    }

    /// Public URL of a linked package category.
    pub(crate) fn category_url(&self, community: &PackageCommunity) -> String {
        format!("{}/category/{}", self.public_url, community.category_slug)
    }

    /// Creates the package category in NodeBB and records it in the registry.
    /// Returns the existing link unchanged when the package is already linked.
    pub(crate) async fn ensure_linked(
        &self,
        packages: &crate::server::PackageBackend,
        package: &Package,
        package_url: &str,
    ) -> Result<PackageCommunity, CommunityError> {
        if let Some(existing) = &package.community {
            return Ok(existing.clone());
        }
        let description = if package.metadata.description.trim().is_empty() {
            format!("Discussion for the {} package. Package page: {package_url}", package.name)
        } else {
            format!("{} Package page: {package_url}", package.metadata.description.trim())
        };
        let category = self
            .client
            .create_category(CreateCategoryRequest {
                name: package.name.clone(),
                description,
                parent_cid: self.packages_parent_cid,
            })
            .await?;
        let community = PackageCommunity { category_id: category.cid, category_slug: category.slug };
        packages
            .set_community(&package.name, community)
            .await
            .map(|package| package.community.expect("set_community stores the link it was given"))
            .map_err(CommunityError::Store)
    }

    /// Live counters; `None` when NodeBB cannot be reached, so a page can
    /// still render the link without numbers.
    pub(crate) async fn stats(&self, community: &PackageCommunity) -> Option<NodebbCategoryStats> {
        self.client.category_stats(community.category_id).await.ok()
    }
}

/// Activity-log entry describing a failed community sync for operators.
pub(crate) fn sync_failure_activity(
    now_unix_seconds: i64,
    actor_subject: Option<&str>,
    package_name: &str,
    error: &CommunityError,
) -> NewRegistryActivity {
    NewRegistryActivity {
        occurred_at_unix_seconds: now_unix_seconds,
        severity: "Warning".to_owned(),
        action: "community_sync_failed".to_owned(),
        message: format!("NodeBB discussion category was not linked: {error}"),
        trace_id: None,
        actor_subject: actor_subject.map(str::to_owned),
        package_name: Some(package_name.to_owned()),
        version: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nodebb_numbers_accept_json_numbers_and_strings() {
        assert_eq!(number(&Some(serde_json::json!(12))), Some(12));
        assert_eq!(number(&Some(serde_json::json!("7"))), Some(7));
        assert_eq!(number(&Some(serde_json::json!(null))), None);
        assert_eq!(number(&None), None);
    }
}
