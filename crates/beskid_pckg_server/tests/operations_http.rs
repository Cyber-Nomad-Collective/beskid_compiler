use axum::body::Body;
use axum::http::{Request, StatusCode};
use beskid_pckg_server::{PckgServerConfig, router};
use http_body_util::BodyExt;
use tower::ServiceExt;

fn admin_config() -> PckgServerConfig {
    PckgServerConfig::default().with_authelia_auth()
}

fn admin_request(method: &str, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("remote-user", "admin")
        .header("remote-groups", "pckg-admins")
        .body(Body::empty())
        .unwrap()
}

async fn json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.expect("response body is readable").to_bytes();
    serde_json::from_slice(&bytes).expect("response is json")
}

#[tokio::test]
async fn super_admin_can_manage_blocked_links_and_read_publish_activity() {
    let app = router(admin_config());

    let added = app
        .clone()
        .oneshot(
            Request::post("/api/admin/blocked-links")
                .header("content-type", "application/json")
                .header("remote-user", "admin")
                .header("remote-groups", "pckg-admins")
                .body(Body::from(r#"{"pattern":"spam.example","note":"abuse"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(added.status(), StatusCode::OK);
    let added = json(added).await;
    assert_eq!(added["item"]["pattern"], "spam.example");

    let blocked = app.clone().oneshot(admin_request("GET", "/api/admin/blocked-links")).await.unwrap();
    assert_eq!(blocked.status(), StatusCode::OK);
    assert_eq!(json(blocked).await[0]["note"], "abuse");

    let created = app
        .clone()
        .oneshot(
            Request::post("/api/packages")
                .header("content-type", "application/json")
                .header("remote-user", "admin")
                .header("remote-groups", "pckg-admins")
                .body(Body::from(r#"{"name":"Audit.Demo","isPublic":true,"submitForReview":false}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);

    let published = app
        .clone()
        .oneshot(
            Request::post("/api/packages/Audit.Demo/versions")
                .header("content-type", "application/json")
                .header("remote-user", "admin")
                .header("remote-groups", "pckg-admins")
                .body(Body::from(serde_json::json!({"version":"1.0.0", "checksumSha256":"a".repeat(64)}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(published.status(), StatusCode::CREATED);

    let activity = app.clone().oneshot(admin_request("GET", "/api/admin/registry-activity?take=50")).await.unwrap();
    assert_eq!(activity.status(), StatusCode::OK);
    assert!(
        json(activity)
            .await
            .as_array()
            .expect("activity array")
            .iter()
            .any(|entry| entry["action"] == "publish_success" && entry["packageName"] == "Audit.Demo")
    );

    let spotlight = app.oneshot(admin_request("POST", "/api/admin/notifications/weekly-spotlight/run")).await.unwrap();
    assert_eq!(spotlight.status(), StatusCode::OK);
    assert_eq!(json(spotlight).await["delivery"], "in_app_only");
}

#[tokio::test]
async fn non_admin_is_forbidden_from_operations_endpoints() {
    let app = router(admin_config());
    let member_request = Request::builder()
        .method("GET")
        .uri("/api/admin/blocked-links")
        .header("remote-user", "member")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(member_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
