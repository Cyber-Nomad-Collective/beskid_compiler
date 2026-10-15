//! NodeBB community integration contract: a public package gets exactly one
//! discussion thread, every successful publish posts one release reply,
//! failures never block creation or publication, the link and live counters
//! are served to the package page, and administrators can reconcile missing
//! threads.

mod support;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use beskid_pckg_server::{
    CommunityConfig, CommunityError, CreateTopicRequest, NodebbClient, NodebbPost, NodebbTopic, NodebbTopicStats,
    PckgServerConfig, ReplyRequest, router,
};
use http_body_util::BodyExt;
use support::{artifact, multipart_publish_request};
use tower::ServiceExt;

#[derive(Default)]
struct FakeNodebb {
    topics: Mutex<Vec<CreateTopicRequest>>,
    replies: Mutex<Vec<ReplyRequest>>,
    fail_topics: Mutex<bool>,
    fail_replies: Mutex<bool>,
    queue_topics: Mutex<bool>,
    stats_available: Mutex<bool>,
}

#[async_trait]
impl NodebbClient for FakeNodebb {
    async fn create_topic(&self, request: CreateTopicRequest) -> Result<NodebbTopic, CommunityError> {
        if *self.fail_topics.lock().unwrap() {
            return Err(CommunityError::Nodebb("503 Service Unavailable".into()));
        }
        if *self.queue_topics.lock().unwrap() {
            return Err(CommunityError::Queued("topic creation".into()));
        }
        let mut topics = self.topics.lock().unwrap();
        topics.push(request.clone());
        let tid = 40 + topics.len() as i64;
        Ok(NodebbTopic { tid, slug: format!("{tid}/{}", request.title.to_ascii_lowercase().replace('.', "-")) })
    }

    async fn reply(&self, request: ReplyRequest) -> Result<(), CommunityError> {
        if *self.fail_replies.lock().unwrap() {
            return Err(CommunityError::Nodebb("timeout".into()));
        }
        self.replies.lock().unwrap().push(request);
        Ok(())
    }

    async fn topic_stats(&self, tid: i64) -> Result<NodebbTopicStats, CommunityError> {
        if !*self.stats_available.lock().unwrap() {
            return Err(CommunityError::Nodebb("timeout".into()));
        }
        Ok(NodebbTopicStats {
            reply_count: tid as u64,
            view_count: tid as u64 * 3,
            recent_posts: vec![
                NodebbPost {
                    pid: 900,
                    author: "octocat".into(),
                    posted_at_unix_seconds: 1_760_000_000,
                    excerpt: "Version 1.1.0 of Acme.Math was published.".into(),
                },
                NodebbPost {
                    pid: 901,
                    author: "beskid-bot".into(),
                    posted_at_unix_seconds: 1_760_000_100,
                    excerpt: "Thanks".into(),
                },
            ],
        })
    }
}

fn community_config() -> CommunityConfig {
    CommunityConfig {
        api_url: "https://community.beskid-lang.org".into(),
        public_url: "https://community.beskid-lang.org/".into(),
        bot_token: "test-token".into(),
        packages_cid: 9,
    }
}

fn app(root: &std::path::Path, nodebb: Arc<FakeNodebb>) -> axum::Router {
    router(
        PckgServerConfig::default()
            .with_authelia_auth()
            .with_artifact_root(root)
            .with_community_client(community_config(), nodebb),
    )
}

fn temp_root(suite: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("pckg-community-{suite}-{}", std::process::id()))
}

async fn json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.expect("body is readable").to_bytes();
    serde_json::from_slice(&bytes).expect("response is JSON")
}

fn create_package(name: &str, is_public: bool) -> Request<Body> {
    Request::post("/api/packages")
        .header("content-type", "application/json")
        .header("remote-user", "owner")
        .body(Body::from(format!(
            r#"{{"name":"{name}","isPublic":{is_public},"description":"Demo package","tags":["demo"],"submitForReview":false}}"#
        )))
        .unwrap()
}

fn reconcile(subject: &str, groups: &str) -> Request<Body> {
    Request::post("/api/admin/community/reconcile")
        .header("remote-user", subject)
        .header("remote-groups", groups)
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn public_package_creation_opens_one_thread_and_each_publish_replies() {
    let root = temp_root("thread");
    let nodebb = Arc::new(FakeNodebb { stats_available: Mutex::new(true), ..FakeNodebb::default() });
    let app = app(&root, nodebb.clone());

    let created = app.clone().oneshot(create_package("Acme.Math", true)).await.unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    {
        let topics = nodebb.topics.lock().unwrap();
        assert_eq!(topics.len(), 1);
        assert_eq!(topics[0].cid, 9);
        assert_eq!(topics[0].title, "Acme.Math");
        assert_eq!(topics[0].tags, vec!["demo".to_owned()]);
        assert!(topics[0].content.starts_with("Demo package"));
        assert!(topics[0].content.contains("https://pckg.beskid-lang.org/packages/Acme.Math"));
        assert!(topics[0].content.contains("https://pckg.beskid-lang.org/packages/Acme.Math/files"));
        assert!(topics[0].content.contains("https://pckg.beskid-lang.org/docs/Acme.Math"));
    }

    for version in ["1.0.0", "1.1.0"] {
        let published = app
            .clone()
            .oneshot(multipart_publish_request("Acme.Math", version, "owner", artifact("Acme.Math", version)))
            .await
            .unwrap();
        assert_eq!(published.status(), StatusCode::CREATED);
    }
    // Re-uploading the same bytes is idempotent and must not announce again.
    let again = app
        .clone()
        .oneshot(multipart_publish_request("Acme.Math", "1.1.0", "owner", artifact("Acme.Math", "1.1.0")))
        .await
        .unwrap();
    assert_eq!(again.status(), StatusCode::OK);
    {
        let replies = nodebb.replies.lock().unwrap();
        assert_eq!(replies.len(), 2, "one reply per created version");
        assert!(replies.iter().all(|reply| reply.tid == 41));
        assert!(replies[0].content.contains("Version 1.0.0 of Acme.Math"));
        assert!(replies[1].content.contains("https://pckg.beskid-lang.org/packages/Acme.Math/files?version=1.1.0"));
        assert!(replies[1].content.contains("https://pckg.beskid-lang.org/docs/Acme.Math?version=1.1.0"));
    }
    assert_eq!(nodebb.topics.lock().unwrap().len(), 1, "publishing never opens another thread");

    let detail =
        json(app.clone().oneshot(Request::get("/api/packages/Acme.Math").body(Body::empty()).unwrap()).await.unwrap())
            .await;
    assert_eq!(detail["package"]["community"]["url"], "https://community.beskid-lang.org/topic/41/acme-math");
    assert_eq!(detail["package"]["community"]["topicId"], 41);
    assert_eq!(detail["package"]["community"]["topicSlug"], "41/acme-math");

    let status = app
        .clone()
        .oneshot(Request::get("/api/packages/Acme.Math/community").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(status.status(), StatusCode::OK);
    let status = json(status).await;
    assert_eq!(status["url"], "https://community.beskid-lang.org/topic/41/acme-math");
    assert_eq!(status["replyCount"], 41);
    assert_eq!(status["viewCount"], 123);
    assert_eq!(status["recentPosts"][0]["pid"], 900);
    assert_eq!(status["recentPosts"][0]["author"], "octocat");
    assert_eq!(status["recentPosts"][0]["postedAtUtc"], "2025-10-09T08:53:20Z");
    assert_eq!(status["recentPosts"][0]["url"], "https://community.beskid-lang.org/post/900");
    assert_eq!(status["recentPosts"][1]["excerpt"], "Thanks");

    // Snapshots are cached for about a minute: a NodeBB outage right after a
    // read keeps serving the cached counters instead of nulls.
    *nodebb.stats_available.lock().unwrap() = false;
    let cached = json(
        app.clone()
            .oneshot(Request::get("/api/packages/Acme.Math/community").body(Body::empty()).unwrap())
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(cached["replyCount"], 41);
    *nodebb.stats_available.lock().unwrap() = true;

    // A package whose thread was never read degrades to null counters when
    // NodeBB is unreachable, but keeps its link.
    assert_eq!(app.clone().oneshot(create_package("Acme.Other", true)).await.unwrap().status(), StatusCode::CREATED);
    *nodebb.stats_available.lock().unwrap() = false;
    let degraded = json(
        app.oneshot(Request::get("/api/packages/Acme.Other/community").body(Body::empty()).unwrap()).await.unwrap(),
    )
    .await;
    assert_eq!(degraded["url"], "https://community.beskid-lang.org/topic/42/acme-other");
    assert!(degraded["replyCount"].is_null(), "counters degrade to null when NodeBB is unreachable");
    assert!(degraded["recentPosts"].as_array().unwrap().is_empty());
    std::fs::remove_dir_all(root).expect("artifact root is removed");
}

#[tokio::test]
async fn private_packages_and_unconfigured_registries_expose_no_community_surface() {
    let root = temp_root("private");
    let nodebb = Arc::new(FakeNodebb::default());
    let app = app(&root, nodebb.clone());

    assert_eq!(app.clone().oneshot(create_package("Secret.Demo", false)).await.unwrap().status(), StatusCode::CREATED);
    let published = app
        .clone()
        .oneshot(multipart_publish_request("Secret.Demo", "1.0.0", "owner", artifact("Secret.Demo", "1.0.0")))
        .await
        .unwrap();
    assert_eq!(published.status(), StatusCode::CREATED);
    assert!(nodebb.topics.lock().unwrap().is_empty(), "private packages never get a public thread");
    assert!(nodebb.replies.lock().unwrap().is_empty(), "private releases are never announced");
    let status = app
        .oneshot(
            Request::get("/api/packages/Secret.Demo/community")
                .header("remote-user", "owner")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(status.status(), StatusCode::NOT_FOUND);

    let plain_root = temp_root("unconfigured");
    let plain = router(PckgServerConfig::default().with_authelia_auth().with_artifact_root(&plain_root));
    assert_eq!(plain.clone().oneshot(create_package("Open.Demo", true)).await.unwrap().status(), StatusCode::CREATED);
    let published = plain
        .clone()
        .oneshot(multipart_publish_request("Open.Demo", "1.0.0", "owner", artifact("Open.Demo", "1.0.0")))
        .await
        .unwrap();
    assert_eq!(published.status(), StatusCode::CREATED);
    let detail = json(
        plain.clone().oneshot(Request::get("/api/packages/Open.Demo").body(Body::empty()).unwrap()).await.unwrap(),
    )
    .await;
    assert!(detail["package"]["community"].is_null());
    let status =
        plain.oneshot(Request::get("/api/packages/Open.Demo/community").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(status.status(), StatusCode::NOT_FOUND);
    std::fs::remove_dir_all(root).ok();
    std::fs::remove_dir_all(plain_root).ok();
}

#[tokio::test]
async fn nodebb_outage_never_blocks_creation_or_publication_and_administrators_reconcile() {
    let root = temp_root("reconcile");
    let nodebb =
        Arc::new(FakeNodebb { fail_topics: Mutex::new(true), fail_replies: Mutex::new(true), ..FakeNodebb::default() });
    let app = app(&root, nodebb.clone());

    assert_eq!(app.clone().oneshot(create_package("Late.Demo", true)).await.unwrap().status(), StatusCode::CREATED);
    assert!(nodebb.topics.lock().unwrap().is_empty());
    let published = app
        .clone()
        .oneshot(multipart_publish_request("Late.Demo", "1.0.0", "owner", artifact("Late.Demo", "1.0.0")))
        .await
        .unwrap();
    assert_eq!(published.status(), StatusCode::CREATED, "publishing succeeds without a thread");
    let detail =
        json(app.clone().oneshot(Request::get("/api/packages/Late.Demo").body(Body::empty()).unwrap()).await.unwrap())
            .await;
    assert!(detail["package"]["community"].is_null(), "no dead link while NodeBB is down");

    assert_eq!(app.clone().oneshot(reconcile("owner", "")).await.unwrap().status(), StatusCode::FORBIDDEN);
    let anonymous = app
        .clone()
        .oneshot(Request::post("/api/admin/community/reconcile").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    *nodebb.fail_topics.lock().unwrap() = false;
    let reconciled = json(app.clone().oneshot(reconcile("admin", "pckg-admins")).await.unwrap()).await;
    assert_eq!(reconciled["linked"][0]["package"], "Late.Demo");
    assert_eq!(reconciled["linked"][0]["url"], "https://community.beskid-lang.org/topic/41/late-demo");
    assert!(reconciled["failed"].as_array().unwrap().is_empty());
    assert!(
        nodebb.topics.lock().unwrap()[0].content.contains("Kind: library"),
        "opening post names the published kind"
    );

    let again = json(app.clone().oneshot(reconcile("admin", "pckg-admins")).await.unwrap()).await;
    assert!(again["linked"].as_array().unwrap().is_empty(), "reconcile is idempotent");
    assert_eq!(nodebb.topics.lock().unwrap().len(), 1, "a package never gets a second thread");

    // Replies still fail: the publish must still succeed and the thread stays linked.
    let published = app
        .clone()
        .oneshot(multipart_publish_request("Late.Demo", "1.1.0", "owner", artifact("Late.Demo", "1.1.0")))
        .await
        .unwrap();
    assert_eq!(published.status(), StatusCode::CREATED);
    assert!(nodebb.replies.lock().unwrap().is_empty());
    *nodebb.fail_replies.lock().unwrap() = false;
    let published = app
        .clone()
        .oneshot(multipart_publish_request("Late.Demo", "1.2.0", "owner", artifact("Late.Demo", "1.2.0")))
        .await
        .unwrap();
    assert_eq!(published.status(), StatusCode::CREATED);
    assert_eq!(nodebb.replies.lock().unwrap().len(), 1);

    let detail =
        json(app.oneshot(Request::get("/api/packages/Late.Demo").body(Body::empty()).unwrap()).await.unwrap()).await;
    assert_eq!(detail["package"]["community"]["topicSlug"], "41/late-demo");
    std::fs::remove_dir_all(root).expect("artifact root is removed");
}

#[tokio::test]
async fn queued_posts_are_not_linked_until_reconcile_succeeds() {
    let root = temp_root("queued");
    let nodebb = Arc::new(FakeNodebb { queue_topics: Mutex::new(true), ..FakeNodebb::default() });
    let app = app(&root, nodebb.clone());

    assert_eq!(app.clone().oneshot(create_package("Queued.Demo", true)).await.unwrap().status(), StatusCode::CREATED);
    let detail = json(
        app.clone().oneshot(Request::get("/api/packages/Queued.Demo").body(Body::empty()).unwrap()).await.unwrap(),
    )
    .await;
    assert!(detail["package"]["community"].is_null(), "a 202 (post queue) never persists a link");
    assert!(nodebb.topics.lock().unwrap().is_empty());

    let activity = json(
        app.clone()
            .oneshot(
                Request::get("/api/admin/registry-activity?take=50")
                    .header("remote-user", "admin")
                    .header("remote-groups", "pckg-admins")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
    )
    .await;
    assert!(
        activity.as_array().unwrap().iter().any(|entry| entry["action"] == "community_sync_failed"
            && entry["message"].as_str().unwrap().contains("post queue")),
        "{activity}"
    );

    *nodebb.queue_topics.lock().unwrap() = false;
    let reconciled = json(app.clone().oneshot(reconcile("admin", "pckg-admins")).await.unwrap()).await;
    assert_eq!(reconciled["linked"][0]["package"], "Queued.Demo");
    assert_eq!(nodebb.topics.lock().unwrap().len(), 1);
    std::fs::remove_dir_all(root).expect("artifact root is removed");
}
