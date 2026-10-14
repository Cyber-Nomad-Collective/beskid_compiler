//! NodeBB community integration contract: a public package gets exactly one
//! discussion category, failures never block package creation, the link and
//! live counters are served to the package page, and administrators can
//! reconcile missing links.

mod support;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use beskid_pckg_server::{
    CommunityConfig, CommunityError, CreateCategoryRequest, NodebbCategory, NodebbCategoryStats, NodebbClient,
    PckgServerConfig, router,
};
use http_body_util::BodyExt;
use support::{artifact, multipart_publish_request};
use tower::ServiceExt;

#[derive(Default)]
struct FakeNodebb {
    created: Mutex<Vec<CreateCategoryRequest>>,
    fail_creation: Mutex<bool>,
    stats_available: Mutex<bool>,
}

#[async_trait]
impl NodebbClient for FakeNodebb {
    async fn create_category(&self, request: CreateCategoryRequest) -> Result<NodebbCategory, CommunityError> {
        if *self.fail_creation.lock().unwrap() {
            return Err(CommunityError::Nodebb("503 Service Unavailable".into()));
        }
        let mut created = self.created.lock().unwrap();
        created.push(request.clone());
        let cid = 100 + created.len() as i64;
        Ok(NodebbCategory { cid, slug: format!("{cid}/{}", request.name.to_ascii_lowercase().replace('.', "-")) })
    }

    async fn category_stats(&self, cid: i64) -> Result<NodebbCategoryStats, CommunityError> {
        if !*self.stats_available.lock().unwrap() {
            return Err(CommunityError::Nodebb("timeout".into()));
        }
        Ok(NodebbCategoryStats { topic_count: cid as u64, post_count: cid as u64 * 2 })
    }
}

fn community_config() -> CommunityConfig {
    CommunityConfig {
        api_url: "http://community:4567".into(),
        public_url: "https://community.beskid-lang.org/".into(),
        admin_token: "test-token".into(),
        packages_parent_cid: 9,
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
            r#"{{"name":"{name}","isPublic":{is_public},"description":"Demo package","submitForReview":false}}"#
        )))
        .unwrap()
}

#[tokio::test]
async fn public_package_creation_links_one_nodebb_category_and_serves_counters() {
    let root = temp_root("link");
    let nodebb = Arc::new(FakeNodebb { stats_available: Mutex::new(true), ..FakeNodebb::default() });
    let app = app(&root, nodebb.clone());

    let created = app.clone().oneshot(create_package("Acme.Math", true)).await.unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let created = nodebb.created.lock().unwrap().clone();
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].name, "Acme.Math");
    assert_eq!(created[0].parent_cid, 9);
    assert!(created[0].description.contains("https://pckg.beskid-lang.org/packages/Acme.Math"));

    let published = app
        .clone()
        .oneshot(multipart_publish_request("Acme.Math", "1.0.0", "owner", artifact("Acme.Math", "1.0.0")))
        .await
        .unwrap();
    assert_eq!(published.status(), StatusCode::CREATED);

    let detail =
        app.clone().oneshot(Request::get("/api/packages/Acme.Math").body(Body::empty()).unwrap()).await.unwrap();
    let detail = json(detail).await;
    assert_eq!(detail["package"]["community"]["url"], "https://community.beskid-lang.org/category/101/acme-math");
    assert_eq!(detail["package"]["community"]["categoryId"], 101);

    let status = app
        .clone()
        .oneshot(Request::get("/api/packages/Acme.Math/community").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(status.status(), StatusCode::OK);
    let status = json(status).await;
    assert_eq!(status["url"], "https://community.beskid-lang.org/category/101/acme-math");
    assert_eq!(status["topicCount"], 101);
    assert_eq!(status["postCount"], 202);

    *nodebb.stats_available.lock().unwrap() = false;
    let degraded =
        app.oneshot(Request::get("/api/packages/Acme.Math/community").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(degraded.status(), StatusCode::OK);
    let degraded = json(degraded).await;
    assert_eq!(degraded["url"], "https://community.beskid-lang.org/category/101/acme-math");
    assert!(degraded["topicCount"].is_null(), "counters degrade to null when NodeBB is unreachable");
    std::fs::remove_dir_all(root).expect("artifact root is removed");
}

#[tokio::test]
async fn private_packages_and_unconfigured_registries_expose_no_community_surface() {
    let root = temp_root("private");
    let nodebb = Arc::new(FakeNodebb::default());
    let app = app(&root, nodebb.clone());

    assert_eq!(app.clone().oneshot(create_package("Secret.Demo", false)).await.unwrap().status(), StatusCode::CREATED);
    assert!(nodebb.created.lock().unwrap().is_empty(), "private packages never get a public category");
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
async fn nodebb_outage_never_blocks_creation_and_administrators_reconcile_missing_links() {
    let root = temp_root("reconcile");
    let nodebb = Arc::new(FakeNodebb { fail_creation: Mutex::new(true), ..FakeNodebb::default() });
    let app = app(&root, nodebb.clone());

    assert_eq!(app.clone().oneshot(create_package("Late.Demo", true)).await.unwrap().status(), StatusCode::CREATED);
    assert!(nodebb.created.lock().unwrap().is_empty());
    let published = app
        .clone()
        .oneshot(multipart_publish_request("Late.Demo", "1.0.0", "owner", artifact("Late.Demo", "1.0.0")))
        .await
        .unwrap();
    assert_eq!(published.status(), StatusCode::CREATED);
    let detail =
        json(app.clone().oneshot(Request::get("/api/packages/Late.Demo").body(Body::empty()).unwrap()).await.unwrap())
            .await;
    assert!(detail["package"]["community"].is_null(), "no dead link while NodeBB is down");

    let forbidden = app
        .clone()
        .oneshot(
            Request::post("/api/admin/community/reconcile").header("remote-user", "owner").body(Body::empty()).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
    let anonymous = app
        .clone()
        .oneshot(Request::post("/api/admin/community/reconcile").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    *nodebb.fail_creation.lock().unwrap() = false;
    let reconciled = app
        .clone()
        .oneshot(
            Request::post("/api/admin/community/reconcile")
                .header("remote-user", "admin")
                .header("remote-groups", "pckg-admins")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reconciled.status(), StatusCode::OK);
    let reconciled = json(reconciled).await;
    assert_eq!(reconciled["linked"][0]["package"], "Late.Demo");
    assert_eq!(reconciled["linked"][0]["url"], "https://community.beskid-lang.org/category/101/late-demo");
    assert!(reconciled["failed"].as_array().unwrap().is_empty());

    let again = app
        .clone()
        .oneshot(
            Request::post("/api/admin/community/reconcile")
                .header("remote-user", "admin")
                .header("remote-groups", "pckg-admins")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(json(again).await["linked"].as_array().unwrap().is_empty(), "reconcile is idempotent");
    assert_eq!(nodebb.created.lock().unwrap().len(), 1, "a package never gets a second category");

    let detail =
        json(app.oneshot(Request::get("/api/packages/Late.Demo").body(Body::empty()).unwrap()).await.unwrap()).await;
    assert_eq!(detail["package"]["community"]["categorySlug"], "101/late-demo");
    std::fs::remove_dir_all(root).expect("artifact root is removed");
}
