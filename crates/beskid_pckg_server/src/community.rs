//! NodeBB community integration.
//!
//! Every public package owns one discussion thread (a NodeBB topic) in the
//! configured "Packages" category on the community site. The registry
//! persists only the topic identity (`tid` + slug); posts and moderation stay
//! in NodeBB. The thread is opened when a public package is created, every
//! successful version publish of a linked package posts a reply announcing
//! the release, and an administrator can repair missing threads through
//! `POST /api/admin/community/reconcile`. Neither package creation nor
//! publication ever fails because NodeBB is unavailable: the failure is
//! recorded in the registry activity log and the package page simply shows no
//! discussion link until the thread exists.
//!
//! Configuration is all-or-nothing (`PCKG_NODEBB_API_URL`,
//! `PCKG_NODEBB_BOT_TOKEN`, `PCKG_NODEBB_PACKAGES_CID`, `PCKG_COMMUNITY_URL`);
//! a partial set is a startup error so a deployment can never half-configure
//! the integration and silently publish dead links.
//!
//! NodeBB facts this module relies on (NodeBB 4.16): the token is a
//! user-bound token of a dedicated bot user sent as `Authorization: Bearer`;
//! NodeBB holds a per-user posting lock (`already-posting`), so every bot
//! write goes through one async mutex; a write answered with HTTP 202 landed
//! in the post queue and carries no usable identity, so it is treated as "not
//! linked" and left to the reconcile path. Thread reads use the public Read
//! API (`GET /api/topic/{tid}`) from the server, cached for about a minute,
//! so browsers never call NodeBB (no CORS).

use std::{
    collections::HashMap,
    env,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use async_trait::async_trait;
use beskid_pckg_store::{NewRegistryActivity, Package, PackageCommunity, StoreError};
use serde::Deserialize;

use crate::server::ServerStartupError;

/// Public origin of the registry web client used inside forum posts.
pub const PACKAGE_SITE_URL: &str = "https://pckg.beskid-lang.org";

/// Validated NodeBB settings taken from the process environment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommunityConfig {
    /// NodeBB origin used for Write/Read API calls.
    pub api_url: String,
    /// Public NodeBB origin used in links shown to users.
    pub public_url: String,
    /// User-bound Write API token of the bot user that opens threads and
    /// posts release replies.
    pub bot_token: String,
    /// The "Packages" category every package thread is created in.
    pub packages_cid: i64,
}

impl CommunityConfig {
    const VARIABLES: [&'static str; 4] =
        ["PCKG_NODEBB_API_URL", "PCKG_NODEBB_BOT_TOKEN", "PCKG_NODEBB_PACKAGES_CID", "PCKG_COMMUNITY_URL"];

    /// Reads the integration settings. Returns `Ok(None)` when none of the
    /// variables is set and an error when only some of them are.
    pub fn from_environment() -> Result<Option<Self>, ServerStartupError> {
        let values = Self::VARIABLES
            .iter()
            .map(|name| env::var(name).ok().map(|value| value.trim().to_owned()).filter(|value| !value.is_empty()))
            .collect::<Vec<_>>();
        Self::from_values(values)
    }

    fn from_values(values: Vec<Option<String>>) -> Result<Option<Self>, ServerStartupError> {
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
        let [api_url, bot_token, packages_cid, public_url] =
            [values[0].clone(), values[1].clone(), values[2].clone(), values[3].clone()].map(Option::unwrap);
        let packages_cid = packages_cid.parse::<i64>().ok().filter(|cid| *cid > 0).ok_or_else(|| {
            ServerStartupError("PCKG_NODEBB_PACKAGES_CID must be a positive NodeBB category id".to_owned())
        })?;
        for (name, value) in [("PCKG_NODEBB_API_URL", &api_url), ("PCKG_COMMUNITY_URL", &public_url)] {
            if !(value.starts_with("http://") || value.starts_with("https://")) {
                return Err(ServerStartupError(format!("{name} must be an absolute http(s) origin")));
            }
        }
        Ok(Some(Self {
            api_url: api_url.trim_end_matches('/').to_owned(),
            public_url: public_url.trim_end_matches('/').to_owned(),
            bot_token,
            packages_cid,
        }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommunityError {
    /// NodeBB rejected or failed the request; the message is operator-facing.
    Nodebb(String),
    /// NodeBB accepted the post into its moderation queue (HTTP 202); no
    /// identity exists yet, so the link is retried by reconcile.
    Queued(String),
    /// NodeBB answered but the payload lacked a usable identity.
    InvalidResponse(String),
    /// The registry could not persist the link.
    Store(StoreError),
}

impl std::fmt::Display for CommunityError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Nodebb(message) => write!(formatter, "NodeBB request failed: {message}"),
            Self::Queued(what) => write!(
                formatter,
                "NodeBB put the {what} into its post queue (HTTP 202); exempt the bot user from the post queue and run the community reconcile"
            ),
            Self::InvalidResponse(message) => write!(formatter, "NodeBB response is invalid: {message}"),
            Self::Store(error) => write!(formatter, "registry could not persist the community link: {error:?}"),
        }
    }
}

/// Thread creation request sent to NodeBB (`POST /api/v3/topics`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateTopicRequest {
    pub cid: i64,
    pub title: String,
    /// Markdown body of the opening post.
    pub content: String,
    pub tags: Vec<String>,
}

/// Reply request sent to NodeBB (`POST /api/v3/topics/{tid}`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplyRequest {
    pub tid: i64,
    /// Markdown body of the reply.
    pub content: String,
}

/// Topic identity returned by NodeBB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodebbTopic {
    pub tid: i64,
    /// NodeBB slug including the numeric prefix, for example `42/acme-math`.
    pub slug: String,
}

/// Public activity counters and the newest posts of a NodeBB topic.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NodebbTopicStats {
    /// Posts in the thread excluding the opening post.
    pub reply_count: u64,
    pub view_count: u64,
    /// Newest posts first, at most [`RECENT_POSTS`].
    pub recent_posts: Vec<NodebbPost>,
}

/// One post of a thread, reduced to what the package page shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodebbPost {
    pub pid: i64,
    pub author: String,
    pub posted_at_unix_seconds: i64,
    /// Plain-text excerpt (Markdown/HTML stripped) of at most [`EXCERPT_CHARS`] characters.
    pub excerpt: String,
}

/// Posts returned per topic snapshot.
pub const RECENT_POSTS: usize = 3;
/// Excerpt length in characters.
pub const EXCERPT_CHARS: usize = 200;
/// How long a topic snapshot is reused before NodeBB is read again.
const SNAPSHOT_TTL: Duration = Duration::from_secs(60);

/// Transport seam over the NodeBB Write/Read APIs. Tests inject a double.
#[async_trait]
pub trait NodebbClient: Send + Sync {
    async fn create_topic(&self, request: CreateTopicRequest) -> Result<NodebbTopic, CommunityError>;
    async fn reply(&self, request: ReplyRequest) -> Result<(), CommunityError>;
    async fn topic_stats(&self, tid: i64) -> Result<NodebbTopicStats, CommunityError>;
}

/// HTTP client over NodeBB's Write API v3 (`/api/v3/topics`) and Read API
/// (`/api/topic/{tid}`).
pub struct HttpNodebbClient {
    http: reqwest::Client,
    api_url: String,
    bot_token: String,
}

impl HttpNodebbClient {
    pub fn new(config: &CommunityConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .user_agent("beskid-pckg-server")
            .build()
            .expect("reqwest client builds with static configuration");
        Self { http, api_url: config.api_url.clone(), bot_token: config.bot_token.clone() }
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
struct TopicPayload {
    tid: Option<serde_json::Value>,
    slug: Option<String>,
    #[serde(default)]
    postcount: Option<serde_json::Value>,
    #[serde(default)]
    viewcount: Option<serde_json::Value>,
    #[serde(default)]
    posts: Vec<PostPayload>,
}

#[derive(Deserialize)]
struct PostPayload {
    pid: Option<serde_json::Value>,
    #[serde(default)]
    timestamp: Option<serde_json::Value>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    user: Option<PostUserPayload>,
}

#[derive(Deserialize)]
struct PostUserPayload {
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    displayname: Option<String>,
}

/// Strips HTML tags and Markdown markers, collapses whitespace and cuts to
/// [`EXCERPT_CHARS`] characters.
pub fn plain_excerpt(content: &str) -> String {
    let mut text = String::with_capacity(content.len());
    let mut in_tag = false;
    for character in content.chars() {
        match character {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                text.push(' ');
            }
            _ if in_tag => {}
            '`' | '*' | '_' | '#' | '>' => text.push(' '),
            _ => text.push(character),
        }
    }
    let text = text
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= EXCERPT_CHARS {
        return collapsed;
    }
    let mut cut = collapsed.chars().take(EXCERPT_CHARS - 1).collect::<String>();
    cut.push('…');
    cut
}

fn recent_posts(posts: Vec<PostPayload>) -> Vec<NodebbPost> {
    let mut posts = posts
        .into_iter()
        .filter_map(|post| {
            let pid = number(&post.pid).filter(|pid| *pid > 0)?;
            let user = post.user.as_ref();
            let author = user
                .and_then(|user| user.displayname.clone().or_else(|| user.username.clone()))
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| "unknown".to_owned());
            Some(NodebbPost {
                pid,
                author,
                posted_at_unix_seconds: number(&post.timestamp).unwrap_or_default().max(0) / 1000,
                excerpt: plain_excerpt(post.content.as_deref().unwrap_or_default()),
            })
        })
        .collect::<Vec<_>>();
    posts.sort_by(|left, right| {
        right.posted_at_unix_seconds.cmp(&left.posted_at_unix_seconds).then(right.pid.cmp(&left.pid))
    });
    posts.truncate(RECENT_POSTS);
    posts
}

fn number(value: &Option<serde_json::Value>) -> Option<i64> {
    match value {
        Some(serde_json::Value::Number(number)) => number.as_i64(),
        Some(serde_json::Value::String(text)) => text.parse().ok(),
        _ => None,
    }
}

async fn write_api<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
    what: &str,
) -> Result<Option<T>, CommunityError> {
    let status = response.status();
    if status == reqwest::StatusCode::ACCEPTED {
        return Err(CommunityError::Queued(what.to_owned()));
    }
    let envelope: WriteApiEnvelope<T> =
        response.json().await.map_err(|error| CommunityError::InvalidResponse(error.to_string()))?;
    if !status.is_success() || envelope.status.code != "ok" {
        return Err(CommunityError::Nodebb(format!(
            "{what} returned {status}: {} {}",
            envelope.status.code, envelope.status.message
        )));
    }
    Ok(envelope.response)
}

#[async_trait]
impl NodebbClient for HttpNodebbClient {
    async fn create_topic(&self, request: CreateTopicRequest) -> Result<NodebbTopic, CommunityError> {
        let response = self
            .http
            .post(format!("{}/api/v3/topics", self.api_url))
            .bearer_auth(&self.bot_token)
            .json(&serde_json::json!({
                "cid": request.cid,
                "title": request.title,
                "content": request.content,
                "tags": request.tags,
            }))
            .send()
            .await
            .map_err(|error| CommunityError::Nodebb(error.to_string()))?;
        let payload: TopicPayload = write_api(response, "topic creation")
            .await?
            .ok_or_else(|| CommunityError::InvalidResponse("missing response".into()))?;
        let tid = number(&payload.tid)
            .filter(|tid| *tid > 0)
            .ok_or_else(|| CommunityError::InvalidResponse("missing topic id".into()))?;
        let slug = payload
            .slug
            .filter(|slug| !slug.trim().is_empty())
            .ok_or_else(|| CommunityError::InvalidResponse("missing topic slug".into()))?;
        Ok(NodebbTopic { tid, slug })
    }

    async fn reply(&self, request: ReplyRequest) -> Result<(), CommunityError> {
        let response = self
            .http
            .post(format!("{}/api/v3/topics/{}", self.api_url, request.tid))
            .bearer_auth(&self.bot_token)
            .json(&serde_json::json!({ "content": request.content }))
            .send()
            .await
            .map_err(|error| CommunityError::Nodebb(error.to_string()))?;
        write_api::<serde_json::Value>(response, "topic reply").await.map(|_| ())
    }

    async fn topic_stats(&self, tid: i64) -> Result<NodebbTopicStats, CommunityError> {
        let response = self
            .http
            .get(format!("{}/api/topic/{tid}", self.api_url))
            .send()
            .await
            .map_err(|error| CommunityError::Nodebb(error.to_string()))?;
        if !response.status().is_success() {
            return Err(CommunityError::Nodebb(format!("topic read returned {}", response.status())));
        }
        let payload: TopicPayload =
            response.json().await.map_err(|error| CommunityError::InvalidResponse(error.to_string()))?;
        let posts = number(&payload.postcount).unwrap_or_default().max(0) as u64;
        Ok(NodebbTopicStats {
            reply_count: posts.saturating_sub(1),
            view_count: number(&payload.viewcount).unwrap_or_default().max(0) as u64,
            recent_posts: recent_posts(payload.posts),
        })
    }
}

/// Registry-side behavior over a [`NodebbClient`].
pub(crate) struct CommunityService {
    client: Arc<dyn NodebbClient>,
    public_url: String,
    packages_cid: i64,
    /// Serializes every bot write: NodeBB rejects concurrent posts of one
    /// user with `already-posting`.
    write_lock: tokio::sync::Mutex<()>,
    snapshots: Mutex<HashMap<i64, (Instant, NodebbTopicStats)>>,
}

impl CommunityService {
    pub(crate) fn new(client: Arc<dyn NodebbClient>, public_url: &str, packages_cid: i64) -> Self {
        Self {
            client,
            public_url: public_url.trim_end_matches('/').to_owned(),
            packages_cid,
            write_lock: tokio::sync::Mutex::new(()),
            snapshots: Mutex::new(HashMap::new()),
        }
    }

    /// Public URL of a linked package thread.
    pub(crate) fn topic_url(&self, community: &PackageCommunity) -> String {
        format!("{}/topic/{}", self.public_url, community.topic_slug)
    }

    /// Public permalink of one post.
    pub(crate) fn post_url(&self, pid: i64) -> String {
        format!("{}/post/{pid}", self.public_url)
    }

    /// Opens the package thread in NodeBB and records it in the registry.
    /// Returns the existing link unchanged when the package is already linked.
    pub(crate) async fn ensure_linked(
        &self,
        packages: &crate::server::PackageBackend,
        package: &Package,
        package_kind: Option<&str>,
    ) -> Result<PackageCommunity, CommunityError> {
        if let Some(existing) = &package.community {
            return Ok(existing.clone());
        }
        let _serialized = self.write_lock.lock().await;
        let topic = self
            .client
            .create_topic(CreateTopicRequest {
                cid: self.packages_cid,
                title: package.name.clone(),
                content: opening_post(package, package_kind),
                tags: package.metadata.tags.iter().take(5).cloned().collect(),
            })
            .await?;
        let community = PackageCommunity { topic_id: topic.tid, topic_slug: topic.slug };
        packages
            .set_community(&package.name, community)
            .await
            .map(|package| package.community.expect("set_community stores the link it was given"))
            .map_err(CommunityError::Store)
    }

    /// Posts the release announcement reply for a linked package.
    pub(crate) async fn announce_version(
        &self,
        community: &PackageCommunity,
        package_name: &str,
        version: &str,
    ) -> Result<(), CommunityError> {
        let _serialized = self.write_lock.lock().await;
        let result = self
            .client
            .reply(ReplyRequest { tid: community.topic_id, content: release_post(package_name, version) })
            .await;
        if result.is_ok() {
            self.snapshots.lock().expect("snapshot cache mutex is not poisoned").remove(&community.topic_id);
        }
        result
    }

    /// Live counters and newest posts, reused for [`SNAPSHOT_TTL`]; `None`
    /// when NodeBB cannot be reached, so a page can still render the link
    /// without numbers.
    pub(crate) async fn stats(&self, community: &PackageCommunity) -> Option<NodebbTopicStats> {
        let tid = community.topic_id;
        if let Some((taken, snapshot)) = self.snapshots.lock().expect("snapshot cache mutex is not poisoned").get(&tid)
            && taken.elapsed() < SNAPSHOT_TTL
        {
            return Some(snapshot.clone());
        }
        let snapshot = self.client.topic_stats(tid).await.ok()?;
        self.snapshots
            .lock()
            .expect("snapshot cache mutex is not poisoned")
            .insert(tid, (Instant::now(), snapshot.clone()));
        Some(snapshot)
    }
}

pub(crate) fn package_page_url(name: &str) -> String {
    format!("{PACKAGE_SITE_URL}/packages/{name}")
}

fn files_page_url(name: &str, version: Option<&str>) -> String {
    match version {
        Some(version) => format!("{PACKAGE_SITE_URL}/packages/{name}/files?version={version}"),
        None => format!("{PACKAGE_SITE_URL}/packages/{name}/files"),
    }
}

fn docs_page_url(name: &str, version: Option<&str>) -> String {
    match version {
        Some(version) => format!("{PACKAGE_SITE_URL}/docs/{name}?version={version}"),
        None => format!("{PACKAGE_SITE_URL}/docs/{name}"),
    }
}

/// Markdown body of the thread's opening post.
fn opening_post(package: &Package, package_kind: Option<&str>) -> String {
    let name = &package.name;
    let mut post = String::new();
    let description = package.metadata.description.trim();
    if !description.is_empty() {
        post.push_str(description);
        post.push_str("\n\n");
    }
    post.push_str(&format!("Kind: {}\n\n", package_kind.unwrap_or("package")));
    post.push_str(&format!(
        "- Package page: {}\n- Files: {}\n- Documentation: {}\n",
        package_page_url(name),
        files_page_url(name, None),
        docs_page_url(name, None)
    ));
    post.push_str("\nThis thread is the discussion for the package; new releases are announced here.");
    post
}

/// Markdown body of a release announcement reply.
fn release_post(name: &str, version: &str) -> String {
    format!(
        "Version {version} of {name} was published.\n\n- Files: {}\n- Documentation: {}\n",
        files_page_url(name, Some(version)),
        docs_page_url(name, Some(version))
    )
}

/// Activity-log entry describing a failed community sync for operators.
pub(crate) fn sync_failure_activity(
    now_unix_seconds: i64,
    actor_subject: Option<&str>,
    package_name: &str,
    version: Option<&str>,
    error: &CommunityError,
) -> NewRegistryActivity {
    NewRegistryActivity {
        occurred_at_unix_seconds: now_unix_seconds,
        severity: "Warning".to_owned(),
        action: "community_sync_failed".to_owned(),
        message: match version {
            Some(version) => format!("NodeBB release announcement for {version} was not posted: {error}"),
            None => format!("NodeBB discussion thread was not linked: {error}"),
        },
        trace_id: None,
        actor_subject: actor_subject.map(str::to_owned),
        package_name: Some(package_name.to_owned()),
        version: version.map(str::to_owned),
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

    #[test]
    fn partial_configuration_fails_closed() {
        let some = |value: &str| Some(value.to_owned());
        assert_eq!(CommunityConfig::from_values(vec![None, None, None, None]).unwrap(), None);
        let error = CommunityConfig::from_values(vec![some("https://c.test"), some("tok"), None, None])
            .expect_err("partial configuration is rejected");
        assert!(error.to_string().contains("PCKG_NODEBB_PACKAGES_CID"));
        assert!(error.to_string().contains("PCKG_COMMUNITY_URL"));
        assert!(
            CommunityConfig::from_values(vec![some("https://c.test"), some("tok"), some("0"), some("https://c.test")])
                .is_err()
        );
        let config = CommunityConfig::from_values(vec![
            some("https://c.test/"),
            some("tok"),
            some("9"),
            some("https://community.test/"),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(config.packages_cid, 9);
        assert_eq!(config.public_url, "https://community.test");
    }

    #[test]
    fn excerpts_strip_markup_and_cut_to_length() {
        assert_eq!(
            plain_excerpt("<p>Hello <strong>world</strong> &amp; `code`</p>\n\n# Title"),
            "Hello world & code Title"
        );
        let long = plain_excerpt(&"word ".repeat(100));
        assert_eq!(long.chars().count(), EXCERPT_CHARS);
        assert!(long.ends_with('…'));
    }

    #[test]
    fn posts_link_to_the_registry_pages() {
        let post = release_post("acme.math", "1.2.0");
        assert!(post.contains("https://pckg.beskid-lang.org/packages/acme.math/files?version=1.2.0"));
        assert!(post.contains("https://pckg.beskid-lang.org/docs/acme.math?version=1.2.0"));
    }
}
