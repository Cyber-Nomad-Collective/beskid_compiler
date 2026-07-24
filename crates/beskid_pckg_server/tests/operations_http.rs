use axum::body::Body;
use axum::http::{Request, StatusCode};
use beskid_pckg_auth::{AuthHubIdentity, issue_pckg_session};
use beskid_pckg_server::{PckgServerConfig, router};
use http_body_util::BodyExt;
use tower::ServiceExt;

fn admin_config() -> PckgServerConfig {
    PckgServerConfig::with_auth_secrets("service-token", "session-secret")
        .with_admin_bootstrap_subject(Some("github:1".to_owned()))
}

fn admin_cookie() -> String {
    format!(
        "pckg_session={}",
        issue_pckg_session(
            &AuthHubIdentity {
                subject: "github:1".to_owned(),
                github_login: "registry-admin".to_owned(),
                hub_session_id: "hub-admin".to_owned(),
            },
            "session-secret",
        )
        .expect("test session issues")
    )
}

async fn json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.expect("response body is readable").to_bytes();
    serde_json::from_slice(&bytes).expect("response is json")
}

#[tokio::test]
async fn super_admin_can_manage_blocked_links_and_read_publish_activity() {
    let app = router(admin_config());
    let cookie = admin_cookie();

    let added = app
        .clone()
        .oneshot(
            Request::post("/api/admin/blocked-links")
                .header("content-type", "application/json")
                .header("cookie", &cookie)
                .body(Body::from(r#"{"pattern":"spam.example","note":"abuse"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(added.status(), StatusCode::OK);
    let added = json(added).await;
    assert_eq!(added["item"]["pattern"], "spam.example");

    let blocked = app
        .clone()
        .oneshot(Request::get("/api/admin/blocked-links").header("cookie", &cookie).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(blocked.status(), StatusCode::OK);
    assert_eq!(json(blocked).await[0]["note"], "abuse");

    let created = app
        .clone()
        .oneshot(
            Request::post("/api/packages")
                .header("content-type", "application/json")
                .header("cookie", &cookie)
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
                .header("cookie", &cookie)
                .body(Body::from(serde_json::json!({"version":"1.0.0", "checksumSha256":"a".repeat(64)}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(published.status(), StatusCode::CREATED);

    let activity = app
        .clone()
        .oneshot(
            Request::get("/api/admin/registry-activity?take=50").header("cookie", &cookie).body(Body::empty()).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(activity.status(), StatusCode::OK);
    assert!(
        json(activity)
            .await
            .as_array()
            .expect("activity array")
            .iter()
            .any(|entry| entry["action"] == "publish_success" && entry["packageName"] == "Audit.Demo")
    );

    let spotlight = app
        .oneshot(
            Request::post("/api/admin/notifications/weekly-spotlight/run")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(spotlight.status(), StatusCode::OK);
    assert_eq!(json(spotlight).await["delivery"], "in_app_only");
}
