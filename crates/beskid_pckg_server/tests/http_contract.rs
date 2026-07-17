use axum::body::Body;
use axum::http::{Request, StatusCode};
use beskid_pckg_auth::{
    AuthHubHandoffClaims, AuthHubIdentity, issue_pckg_session, sign_auth_hub_handoff,
};
use beskid_pckg_server::{PckgServerConfig, router};
use http_body_util::BodyExt;
use sha2::Digest;
use std::{
    fs,
    io::{Cursor, Write},
};
use tower::ServiceExt;
use zip::{ZipWriter, write::SimpleFileOptions};

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

    let my_packages = app
        .clone()
        .oneshot(
            Request::get("/api/packages?owner=me")
                .header("cookie", &owner_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(my_packages.status(), StatusCode::OK);
    assert_eq!(response_body(my_packages).await[0]["name"], "Public.Demo");

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
async fn public_package_reviews_are_upserted_by_github_subject_and_reject_blocked_links() {
    let app = router(authenticated_config());
    let owner_cookie = format!("pckg_session={}", package_session("github:1"));
    let reviewer_cookie = format!("pckg_session={}", package_session("github:2"));
    let created = app
        .clone()
        .oneshot(
            Request::post("/api/packages")
                .header("content-type", "application/json")
                .header("cookie", &owner_cookie)
                .body(Body::from(
                    r#"{"name":"Review.Demo","isPublic":true,"submitForReview":false}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let posted = app
        .clone()
        .oneshot(
            Request::post("/api/packages/Review.Demo/community-reviews")
                .header("content-type", "application/json")
                .header("cookie", &reviewer_cookie)
                .body(Body::from(
                    r#"{"rating":5,"comment":"Useful registry package."}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(posted.status(), StatusCode::CREATED);
    let posted = response_body(posted).await;
    let updated = app
        .clone()
        .oneshot(
            Request::post("/api/packages/Review.Demo/community-reviews")
                .header("content-type", "application/json")
                .header("cookie", &reviewer_cookie)
                .body(Body::from(
                    r#"{"rating":4,"comment":"Useful after revision."}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::CREATED);
    let listed = app
        .clone()
        .oneshot(
            Request::get("/api/packages/Review.Demo/community-reviews")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(listed.status(), StatusCode::OK);
    assert_eq!(
        response_body(listed).await,
        serde_json::json!([{"id": posted["id"], "author":"github:2","rating":4,"comment":"Useful after revision.","createdAtUtc": posted["createdAtUtc"]}])
    );
}

#[tokio::test]
async fn current_owner_package_filter_does_not_allow_anonymous_catalog_probing() {
    let response = router(PckgServerConfig::default())
        .oneshot(
            Request::get("/api/packages?owner=me")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn publisher_catalog_uses_profiled_github_subjects_and_hides_private_packages() {
    let app = router(authenticated_config());
    let publisher_cookie = format!("pckg_session={}", package_session("github:100"));
    let unprofiled_cookie = format!("pckg_session={}", package_session("github:200"));

    let profile = app
        .clone()
        .oneshot(
            Request::put("/api/community/profiles/me")
                .header("content-type", "application/json")
                .header("cookie", &publisher_cookie)
                .body(Body::from(
                    r#"{"displayName":"Registry Team","bio":"Maintainers","socialLinks":["https://example.test"]}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(profile.status(), StatusCode::OK);

    for (name, is_public, cookie) in [
        ("Public.Profiled", true, &publisher_cookie),
        ("Private.Profiled", false, &publisher_cookie),
        ("Public.Unprofiled", true, &unprofiled_cookie),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/packages")
                    .header("content-type", "application/json")
                    .header("cookie", cookie)
                    .body(Body::from(
                        serde_json::json!({
                            "name": name,
                            "isPublic": is_public,
                            "submitForReview": false,
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    let directory = app
        .clone()
        .oneshot(Request::get("/api/publishers").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(directory.status(), StatusCode::OK);
    assert_eq!(
        response_body(directory).await,
        serde_json::json!([{
            "subject": "github:100",
            "displayName": "Registry Team",
            "bio": "Maintainers",
            "socialLinks": ["https://example.test"],
            "isPublisherVerified": false,
            "packageCount": 1,
        }])
    );

    let packages = app
        .clone()
        .oneshot(
            Request::get("/api/publishers/github:100/packages")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(packages.status(), StatusCode::OK);
    assert_eq!(response_body(packages).await[0]["name"], "Public.Profiled");

    for path in [
        "/api/publishers/github:not-a-number/packages",
        "/api/publishers/github:200/packages",
    ] {
        let response = app
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
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

#[tokio::test]
async fn api_key_management_requires_an_auth_hub_session() {
    let app = router(PckgServerConfig::default());
    for request in [
        Request::get("/api/api-keys").body(Body::empty()).unwrap(),
        Request::post("/api/api-keys")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"name":"CI","scopes":["publish"]}"#))
            .unwrap(),
        Request::delete("/api/api-keys/00000000-0000-0000-0000-000000000000")
            .body(Body::empty())
            .unwrap(),
    ] {
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response_body(response).await,
            serde_json::json!({"message": "authentication required"})
        );
    }
}

#[tokio::test]
async fn administration_never_bootstraps_privilege_or_discloses_admin_state() {
    let app = router(authenticated_config());
    let member_cookie = format!("pckg_session={}", package_session("github:42"));

    let role_list = app
        .clone()
        .oneshot(
            Request::get("/api/admin/roles")
                .header("cookie", &member_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(role_list.status(), StatusCode::SERVICE_UNAVAILABLE);

    let anonymous = app
        .oneshot(
            Request::get("/api/admin/roles")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn administration_ui_contract_routes_require_a_session() {
    let app = router(PckgServerConfig::default());
    for request in [
        Request::get("/api/admin/users")
            .body(Body::empty())
            .unwrap(),
        Request::patch("/api/admin/users/github%3A42")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"roles":["Moderator"],"publisherVerified":true}"#,
            ))
            .unwrap(),
        Request::get("/api/admin/permissions")
            .body(Body::empty())
            .unwrap(),
        Request::post("/api/admin/permissions")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"subject":"github:42","resource":"package:demo","capability":"moderate"}"#,
            ))
            .unwrap(),
    ] {
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
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

#[tokio::test]
async fn package_lifecycle_lists_versions_and_hides_delete_from_non_owners() {
    let app = router(authenticated_config());
    let owner_cookie = format!("pckg_session={}", package_session("github:71"));
    let other_cookie = format!("pckg_session={}", package_session("github:72"));
    let create = app
        .clone()
        .oneshot(
            Request::post("/api/packages")
                .header("content-type", "application/json")
                .header("cookie", &owner_cookie)
                .body(Body::from(
                    r#"{"name":"Lifecycle.Demo","isPublic":false,"submitForReview":false}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let publish = app
        .clone()
        .oneshot(
            Request::post("/api/packages/Lifecycle.Demo/versions")
                .header("content-type", "application/json")
                .header("cookie", &owner_cookie)
                .body(Body::from(
                    serde_json::json!({"version":"1.0.0", "checksumSha256":"b".repeat(64)})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(publish.status(), StatusCode::CREATED);

    let hidden_list = app
        .clone()
        .oneshot(
            Request::get("/api/packages/Lifecycle.Demo/versions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(hidden_list.status(), StatusCode::NOT_FOUND);
    let versions = app
        .clone()
        .oneshot(
            Request::get("/api/packages/Lifecycle.Demo/versions")
                .header("cookie", &owner_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(versions.status(), StatusCode::OK);
    assert_eq!(response_body(versions).await[0]["version"], "1.0.0");

    let hidden_delete = app
        .clone()
        .oneshot(
            Request::delete("/api/packages/Lifecycle.Demo")
                .header("cookie", &other_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(hidden_delete.status(), StatusCode::NOT_FOUND);
    let deleted = app
        .clone()
        .oneshot(
            Request::delete("/api/packages/Lifecycle.Demo")
                .header("cookie", &owner_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::OK);
    assert_eq!(response_body(deleted).await["success"], true);
    let absent = app
        .oneshot(
            Request::get("/api/packages/Lifecycle.Demo")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(absent.status(), StatusCode::NOT_FOUND);
}

fn authenticated_config() -> PckgServerConfig {
    PckgServerConfig::with_auth_secrets("auth-hub-test-secret", "pckg-session-test-secret")
}

#[tokio::test]
async fn workspace_publish_provisions_members_and_publishes_registry_versions() {
    let artifact_root = std::env::temp_dir().join(format!(
        "beskid-pckg-workspace-provision-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&artifact_root);
    let app = router(authenticated_config().with_artifact_root(&artifact_root));
    let cookie = format!("pckg_session={}", package_session("github:501"));
    let bundle = workspace_bundle();
    let response = app
        .clone()
        .oneshot(multipart_request(
            "/api/workspaces/publish",
            "artifact",
            "workspace.zip",
            &bundle,
            &cookie,
        ))
        .await
        .unwrap();
    let status = response.status();
    let body = response_body(response).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["success"], true);
    assert_eq!(body["workspaceName"], "DemoWorkspace");
    assert_eq!(body["packages"].as_array().unwrap().len(), 2);

    for name in ["Workspace.Foundation", "Workspace.Consumer"] {
        let detail = app
            .clone()
            .oneshot(
                Request::get(format!("/api/packages/{name}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(detail.status(), StatusCode::OK);
        assert_eq!(response_body(detail).await["latestVersion"], "0.0.1");
    }
    let _ = fs::remove_dir_all(artifact_root);
}

#[tokio::test]
async fn workspace_publish_rolls_back_every_member_when_a_later_member_is_invalid() {
    let app = router(authenticated_config());
    let cookie = format!("pckg_session={}", package_session("github:511"));
    let response = app
        .clone()
        .oneshot(multipart_request(
            "/api/workspaces/publish",
            "artifact",
            "workspace.zip",
            &workspace_bundle_with_invalid_later_member(),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    for name in ["Workspace.Foundation", "Workspace.Consumer"] {
        let detail = app
            .clone()
            .oneshot(
                Request::get(format!("/api/packages/{name}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            detail.status(),
            StatusCode::NOT_FOUND,
            "{name} leaked after rollback"
        );
    }
}

#[tokio::test]
async fn concurrent_workspace_publish_never_overwrites_an_immutable_artifact() {
    let artifact_root = std::env::temp_dir().join(format!(
        "beskid-pckg-workspace-atomicity-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&artifact_root);
    // Separate in-memory registry adapters model concurrent registry workers:
    // they share artifact storage, but neither can observe the other's version
    // reservation before its own publish reaches the immutable object key.
    let app_one = router(authenticated_config().with_artifact_root(&artifact_root));
    let app_two = router(authenticated_config().with_artifact_root(&artifact_root));
    let cookie = format!("pckg_session={}", package_session("github:512"));
    let first = workspace_bundle_with_member_source("// first publisher bytes");
    let second = workspace_bundle_with_member_source("// second publisher bytes");
    let (left, right) = tokio::join!(
        app_one.clone().oneshot(multipart_request(
            "/api/workspaces/publish",
            "artifact",
            "workspace.zip",
            &first,
            &cookie,
        )),
        app_two.clone().oneshot(multipart_request(
            "/api/workspaces/publish",
            "artifact",
            "workspace.zip",
            &second,
            &cookie,
        )),
    );
    let left = left.unwrap();
    let right = right.unwrap();
    assert!(
        [left.status(), right.status()].contains(&StatusCode::OK),
        "one publisher must win"
    );
    assert!(
        [left.status(), right.status()].contains(&StatusCode::CONFLICT),
        "different immutable artifacts must not both publish"
    );

    let winner = if left.status() == StatusCode::OK {
        app_one
    } else {
        app_two
    };
    let version = winner
        .clone()
        .oneshot(
            Request::get("/api/packages/Workspace.Foundation/versions")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let checksum = response_body(version).await[0]["checksumSha256"]
        .as_str()
        .unwrap()
        .to_owned();
    let artifact = winner
        .oneshot(
            Request::get("/api/packages/Workspace.Foundation/versions/0.0.1/download")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(artifact.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(artifact.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(
        format!("{:x}", sha2::Sha256::digest(&bytes)),
        checksum,
        "durable artifact bytes must match the committed version checksum"
    );
    let _ = fs::remove_dir_all(artifact_root);
}

#[tokio::test]
async fn package_review_queue_enforces_auth_hub_owner_policy_and_records_actions() {
    let app = router(authenticated_config());
    let owner_cookie = format!("pckg_session={}", package_session("github:601"));
    let stranger_cookie = format!("pckg_session={}", package_session("github:602"));
    let created = app
        .clone()
        .oneshot(
            Request::post("/api/packages")
                .header("content-type", "application/json")
                .header("cookie", &owner_cookie)
                .body(Body::from(
                    r#"{"name":"Review.Queue","isPublic":true,"submitForReview":false}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);

    let submitted = app
        .clone()
        .oneshot(
            Request::post("/api/packages/Review.Queue/review-requests")
                .header("content-type", "application/json")
                .header("cookie", &owner_cookie)
                .body(Body::from(r#"{"reason":"Please review this package."}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(submitted.status(), StatusCode::CREATED);
    let review_id = response_body(submitted).await["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let hidden = app
        .clone()
        .oneshot(
            Request::get("/api/packages/reviews")
                .header("cookie", &stranger_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(hidden.status(), StatusCode::OK);
    assert!(response_body(hidden).await.as_array().unwrap().is_empty());

    let action = app
        .clone()
        .oneshot(
            Request::post(format!("/api/packages/reviews/{review_id}/actions"))
                .header("content-type", "application/json")
                .header("cookie", &owner_cookie)
                .body(Body::from(r#"{"action":"approved","notes":"looks good"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(action.status(), StatusCode::OK);
    assert_eq!(response_body(action).await["status"], "approved");
}

fn workspace_bundle() -> Vec<u8> {
    let entries = [
        (
            "Workspace.proj",
            "workspace {\n  name = \"DemoWorkspace\"\n}\n\nmember \"foundation\" {\n  path = \"foundation\"\n}\n\nmember \"consumer\" {\n  path = \"consumer\"\n}",
        ),
        (
            "workspace.package.json",
            r#"{"schema":"beskid.workspace.package.v1","members":{"foundation":{"package":"Workspace.Foundation"},"consumer":{"package":"Workspace.Consumer"}}}"#,
        ),
        (
            "foundation/Project.proj",
            "project { name = \"Workspace.Foundation\" }",
        ),
        ("foundation/src/Prelude.bd", "// foundation"),
        (
            "consumer/Project.proj",
            "project { name = \"Workspace.Consumer\" }",
        ),
        ("consumer/src/Main.bd", "// consumer"),
    ];
    let mut output = Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut output);
        for (path, contents) in entries {
            zip.start_file(path, SimpleFileOptions::default()).unwrap();
            zip.write_all(contents.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }
    output.into_inner()
}

fn workspace_bundle_with_invalid_later_member() -> Vec<u8> {
    let mut bundle = workspace_bundle();
    let mut archive = zip::ZipArchive::new(Cursor::new(&bundle)).unwrap();
    let mut output = Cursor::new(Vec::new());
    {
        let mut writer = ZipWriter::new(&mut output);
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).unwrap();
            if entry.name() == "consumer/src/Main.bd" {
                continue;
            }
            writer
                .start_file(entry.name(), SimpleFileOptions::default())
                .unwrap();
            std::io::copy(&mut entry, &mut writer).unwrap();
        }
        writer.finish().unwrap();
    }
    bundle = output.into_inner();
    bundle
}

fn workspace_bundle_with_member_source(source: &str) -> Vec<u8> {
    let mut archive = zip::ZipArchive::new(Cursor::new(workspace_bundle())).unwrap();
    let mut output = Cursor::new(Vec::new());
    {
        let mut writer = ZipWriter::new(&mut output);
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).unwrap();
            writer
                .start_file(entry.name(), SimpleFileOptions::default())
                .unwrap();
            if entry.name() == "foundation/src/Prelude.bd" {
                writer.write_all(source.as_bytes()).unwrap();
            } else {
                std::io::copy(&mut entry, &mut writer).unwrap();
            }
        }
        writer.finish().unwrap();
    }
    output.into_inner()
}

fn multipart_request(
    path: &str,
    field: &str,
    filename: &str,
    bytes: &[u8],
    cookie: &str,
) -> Request<Body> {
    let boundary = "pckg-workspace-test";
    let mut body = format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{field}\"; filename=\"{filename}\"\r\nContent-Type: application/zip\r\n\r\n").into_bytes();
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    Request::post(path)
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .header("cookie", cookie)
        .body(Body::from(body))
        .unwrap()
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
async fn auth_hub_finish_rejects_ambiguous_legacy_identity_subjects() {
    let token = handoff_token("pckg", "legacy-user-1", "octocat", "hub-1");
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
    let token = handoff_token("pckg", "github:1", "octocat", "hub-1");
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
        serde_json::json!({"subject": "github:1", "githubLogin": "octocat", "hubSessionId": "hub-1"})
    );
}
