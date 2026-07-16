use axum::body::Body;
use axum::http::{Request, StatusCode};
use beskid_pckg_auth::{
    AuthHubHandoffClaims, AuthHubIdentity, issue_pckg_session, sign_auth_hub_handoff,
};
use beskid_pckg_server::{PckgServerConfig, router};
use http_body_util::BodyExt;
use std::fs;
use tower::ServiceExt;

async fn response_body(response: axum::response::Response) -> serde_json::Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body is readable")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("response is JSON")
}

#[tokio::test]
async fn health_endpoints_report_live_and_ready() {
    let app = router(PckgServerConfig::default());

    for path in ["/health/live", "/health/ready"] {
        let response = app
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response_body(response).await,
            serde_json::json!({"status": "ok"})
        );
    }
}

#[tokio::test]
async fn package_index_search_and_detail_return_persisted_public_data() {
    let app = router(authenticated_config());
    let owner_cookie = format!("pckg_session={}", package_session("github:1"));
    let create = app
        .clone()
        .oneshot(
            Request::post("/api/packages")
                .header("content-type", "application/json")
                .header("cookie", &owner_cookie)
                .body(Body::from(
                    r#"{"name":"Public.Demo","isPublic":true,"submitForReview":false}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let package_id = response_body(create).await["id"]
        .as_str()
        .unwrap()
        .to_owned();

    for version in ["1.0.0", "2.0.0"] {
        let published = app
            .clone()
            .oneshot(
                Request::post("/api/packages/Public.Demo/versions")
                    .header("content-type", "application/json")
                    .header("cookie", &owner_cookie)
                    .body(Body::from(
                        serde_json::json!({"version": version, "checksumSha256": "a".repeat(64)})
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(published.status(), StatusCode::CREATED);
    }

    let list = app
        .clone()
        .oneshot(
            Request::get("/api/packages?limit=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    assert_eq!(response_body(list).await[0]["name"], "Public.Demo");

    let search = app
        .clone()
        .oneshot(
            Request::get("/api/search?q=public")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(search.status(), StatusCode::OK);
    assert_eq!(
        response_body(search).await[0]["package"]["name"],
        "Public.Demo"
    );

    let detail = app
        .oneshot(
            Request::get(format!("/api/packages/{package_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(detail.status(), StatusCode::OK);
    let detail = response_body(detail).await;
    assert_eq!(detail["versions"].as_array().unwrap().len(), 2);
    assert_eq!(detail["latestVersion"], "2.0.0");
}

#[tokio::test]
async fn web_root_serves_assets_and_uses_index_for_client_routes() {
    let web_root = std::env::temp_dir().join(format!("beskid-pckg-web-{}", std::process::id()));
    fs::create_dir_all(web_root.join("assets")).expect("web root is created");
    fs::write(web_root.join("index.html"), "<main>pckg</main>").expect("index is written");
    fs::write(web_root.join("assets/app.js"), "console.log('pckg')").expect("asset is written");

    let app = router(PckgServerConfig::default().with_web_root(&web_root));
    let asset = app
        .clone()
        .oneshot(Request::get("/assets/app.js").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(asset.status(), StatusCode::OK);
    assert_eq!(
        asset.into_body().collect().await.unwrap().to_bytes(),
        "console.log('pckg')"
    );

    let client_route = app
        .oneshot(
            Request::get("/dashboard/packages/my")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(client_route.status(), StatusCode::OK);
    assert_eq!(
        client_route.into_body().collect().await.unwrap().to_bytes(),
        "<main>pckg</main>"
    );

    fs::remove_dir_all(web_root).expect("temporary web root is removed");
}

#[tokio::test]
async fn package_mutations_require_an_authenticated_session() {
    let response = router(PckgServerConfig::default())
        .oneshot(
            Request::post("/api/packages")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"name":"Private.Demo","isPublic":false,"submitForReview":false}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response_body(response).await,
        serde_json::json!({"message": "authentication required"})
    );
}

fn package_session(subject: &str) -> String {
    issue_pckg_session(
        &AuthHubIdentity {
            subject: subject.to_owned(),
            github_login: "octocat".to_owned(),
            hub_session_id: "hub-1".to_owned(),
        },
        "pckg-session-test-secret",
    )
    .expect("test session issues")
}

#[tokio::test]
async fn package_mutations_are_owned_by_the_verified_auth_hub_subject() {
    let app = router(authenticated_config());
    let owner_cookie = format!("pckg_session={}", package_session("github:1"));
    let other_cookie = format!("pckg_session={}", package_session("github:2"));
    let create = app
        .clone()
        .oneshot(
            Request::post("/api/packages")
                .header("content-type", "application/json")
                .header("cookie", &owner_cookie)
                .body(Body::from(
                    r#"{"name":"Private.Demo","isPublic":false,"submitForReview":false}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    assert_eq!(response_body(create).await["ownerUserId"], "github:1");

    let hidden = app
        .clone()
        .oneshot(
            Request::get("/api/packages/Private.Demo")
                .header("cookie", &other_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(hidden.status(), StatusCode::NOT_FOUND);

    let checksum = "a".repeat(64);
    let publish_body =
        serde_json::json!({"version": "1.0.0", "checksumSha256": checksum}).to_string();
    let publish = app
        .clone()
        .oneshot(
            Request::post("/api/packages/Private.Demo/versions")
                .header("content-type", "application/json")
                .header("cookie", &owner_cookie)
                .body(Body::from(publish_body.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(publish.status(), StatusCode::CREATED);

    let retry = app
        .clone()
        .oneshot(
            Request::post("/api/packages/Private.Demo/versions")
                .header("content-type", "application/json")
                .header("cookie", &owner_cookie)
                .body(Body::from(publish_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(retry.status(), StatusCode::OK);

    let forbidden = app
        .oneshot(
            Request::post("/api/packages/Private.Demo/versions/1.0.0/yank")
                .header("cookie", &other_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(forbidden.status(), StatusCode::NOT_FOUND);
}

fn authenticated_config() -> PckgServerConfig {
    PckgServerConfig::with_auth_secrets("auth-hub-test-secret", "pckg-session-test-secret")
}

fn handoff_token(app: &str, subject: &str, login: &str, sid: &str) -> String {
    sign_auth_hub_handoff(
        &AuthHubHandoffClaims {
            app: app.to_owned(),
            subject: subject.to_owned(),
            login: login.to_owned(),
            sid: sid.to_owned(),
            expires_at: 4_102_444_800,
        },
        "auth-hub-test-secret",
    )
    .expect("test token signs")
}

#[tokio::test]
async fn auth_hub_finish_rejects_handoffs_for_another_app() {
    let token = handoff_token("tracker", "user-1", "octocat", "hub-1");
    let response = router(authenticated_config())
        .oneshot(
            Request::get(format!("/api/auth/hub-finish?handoff={token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response_body(response).await,
        serde_json::json!({"message": "invalid handoff"})
    );
}

#[tokio::test]
async fn auth_hub_finish_rejects_handoffs_without_a_subject() {
    let token = handoff_token("pckg", "", "octocat", "hub-1");
    let response = router(authenticated_config())
        .oneshot(
            Request::get(format!("/api/auth/hub-finish?handoff={token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response_body(response).await,
        serde_json::json!({"message": "invalid handoff"})
    );
}

#[tokio::test]
async fn auth_hub_finish_sets_an_http_only_session_that_session_endpoint_reads() {
    let token = handoff_token("pckg", "user-1", "octocat", "hub-1");
    let app = router(authenticated_config());
    let finish = app
        .clone()
        .oneshot(
            Request::get(format!("/api/auth/hub-finish?handoff={token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(finish.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        finish.headers().get("location").unwrap(),
        "/dashboard/packages/my"
    );
    let session_cookie = finish
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(session_cookie.starts_with("pckg_session="));
    assert!(session_cookie.contains("HttpOnly"));

    let session = app
        .oneshot(
            Request::get("/api/auth/session")
                .header("cookie", session_cookie.split(';').next().unwrap())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(session.status(), StatusCode::OK);
    assert_eq!(
        response_body(session).await,
        serde_json::json!({"subject": "user-1", "githubLogin": "octocat", "hubSessionId": "hub-1"})
    );
}
