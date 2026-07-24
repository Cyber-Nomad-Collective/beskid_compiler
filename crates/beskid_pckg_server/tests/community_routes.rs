#[path = "../src/community_routes.rs"]
mod community_routes;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use beskid_pckg_auth::{AuthHubIdentity, issue_pckg_session};
use beskid_pckg_community::{Board, BoardId, Principal, Role, Subject};
use http_body_util::BodyExt;
use tower::ServiceExt;

async fn json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn authenticated_cookie(subject: &str) -> String {
    let token = issue_pckg_session(
        &AuthHubIdentity {
            subject: subject.to_owned(),
            github_login: "octocat".to_owned(),
            hub_session_id: "hub-1".to_owned(),
        },
        "test-session-secret",
    )
    .unwrap();
    format!("pckg_session={token}")
}

#[tokio::test]
async fn community_mutations_require_an_auth_hub_session() {
    let state = community_routes::CommunityState::with_session_secret("test-session-secret");
    let response = community_routes::router(state)
        .oneshot(
            Request::post("/boards/general/posts")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"title":"Hello","content":"World"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(json(response).await, serde_json::json!({"message": "authentication required"}));
}

#[tokio::test]
async fn authenticated_session_can_create_a_board_post() {
    let state = community_routes::CommunityState::with_session_secret("test-session-secret");
    state.service().lock().unwrap().add_board(Board::new(BoardId::new("general").unwrap(), "General"));

    let response = community_routes::router(state)
        .oneshot(
            Request::post("/boards/general/posts")
                .header("content-type", "application/json")
                .header("cookie", authenticated_cookie("github:1"))
                .body(Body::from(r#"{"title":"Hello","content":"World"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(
        json(response).await,
        serde_json::json!({
            "id": 1,
            "boardId": "general",
            "author": "github:1",
            "title": "Hello",
            "content": "World",
            "score": 0
        })
    );
}

#[tokio::test]
async fn only_a_moderator_can_lock_and_unlock_a_board() {
    let state = community_routes::CommunityState::with_session_secret("test-session-secret");
    state.service().lock().unwrap().add_board(Board::new(BoardId::new("general").unwrap(), "General"));

    let app = community_routes::router(state.clone());
    let member = app
        .clone()
        .oneshot(
            Request::post("/boards/general/moderation/lock")
                .header("content-type", "application/json")
                .header("cookie", authenticated_cookie("github:1"))
                .body(Body::from(r#"{"locked":true}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(member.status(), StatusCode::FORBIDDEN);

    state.grant_test_moderator("github:2");
    let locked = app
        .clone()
        .oneshot(
            Request::post("/boards/general/moderation/lock")
                .header("content-type", "application/json")
                .header("cookie", authenticated_cookie("github:2"))
                .body(Body::from(r#"{"locked":true}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(locked.status(), StatusCode::OK);
    assert_eq!(json(locked).await, serde_json::json!({"success":true,"message":"Board locked."}));

    let blocked = app
        .clone()
        .oneshot(
            Request::post("/boards/general/posts")
                .header("content-type", "application/json")
                .header("cookie", authenticated_cookie("github:1"))
                .body(Body::from(r#"{"title":"Hello","content":"World"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(blocked.status(), StatusCode::FORBIDDEN);

    let unlocked = app
        .oneshot(
            Request::post("/boards/general/moderation/lock")
                .header("content-type", "application/json")
                .header("cookie", authenticated_cookie("github:2"))
                .body(Body::from(r#"{"locked":false}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unlocked.status(), StatusCode::OK);
    assert_eq!(json(unlocked).await, serde_json::json!({"success":true,"message":"Board unlocked."}));
}

#[tokio::test]
async fn auth_hub_session_reads_its_own_profile_without_exposing_another_subject() {
    let state = community_routes::CommunityState::with_session_secret("test-session-secret");
    state
        .service()
        .lock()
        .unwrap()
        .upsert_profile(beskid_pckg_community::Profile::new(Subject::new("github:1").unwrap(), "Octocat"));

    let response = community_routes::router(state)
        .oneshot(
            Request::get("/profiles/me")
                .header("cookie", authenticated_cookie("github:1"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json(response).await,
        serde_json::json!({
            "subject": "github:1",
            "display_name": "Octocat",
            "bio": "",
            "social_links": [],
            "is_publisher_verified": false
        })
    );
}

#[tokio::test]
async fn publisher_follow_count_is_public_but_follower_identity_is_not_disclosed() {
    let state = community_routes::CommunityState::with_session_secret("test-session-secret");
    let publisher = Subject::new("github:9").unwrap();
    state
        .service()
        .lock()
        .unwrap()
        .toggle_publisher_follow(&Principal::auth_hub(Subject::new("github:1").unwrap(), [Role::User]), &publisher)
        .unwrap();

    let response = community_routes::router(state)
        .oneshot(Request::get("/publisher-follows/github:9/count").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json(response).await, serde_json::json!({ "count": 1 }));
}

#[tokio::test]
async fn notification_actions_and_bulk_read_are_recipient_scoped() {
    let app = community_routes::router(community_routes::CommunityState::with_session_secret("test-session-secret"));
    let owner_cookie = authenticated_cookie("github:1");
    let other_cookie = authenticated_cookie("github:2");

    let created = app
        .clone()
        .oneshot(Request::post("/notifications/test").header("cookie", &owner_cookie).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    assert_eq!(json(created).await, serde_json::json!({"id": 1}));

    let hidden = app
        .clone()
        .oneshot(
            Request::post("/notifications/1/actions")
                .header("cookie", &other_cookie)
                .header("content-type", "application/json")
                .body(Body::from(r#"{"action":"dismiss"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(hidden.status(), StatusCode::NOT_FOUND);

    let marked = app
        .clone()
        .oneshot(
            Request::post("/notifications/mark-all-read").header("cookie", &owner_cookie).body(Body::empty()).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(marked.status(), StatusCode::OK);
    assert_eq!(json(marked).await, serde_json::json!({"updated": 1}));

    let notifications = app
        .oneshot(Request::get("/notifications").header("cookie", &owner_cookie).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(notifications.status(), StatusCode::OK);
    assert_eq!(
        json(notifications).await,
        serde_json::json!([{
            "id": 1,
            "recipient": "github:1",
            "scope": "system",
            "actor": "github:1",
            "post_id": null,
            "comment_id": null,
            "is_read": true
        }])
    );
}

#[tokio::test]
async fn typed_notification_preferences_round_trip_for_authenticated_subject() {
    let app = community_routes::router(community_routes::CommunityState::with_session_secret("test-session-secret"));
    let cookie = authenticated_cookie("github:1");
    let updated = app
        .clone()
        .oneshot(
            Request::put("/notification-preferences")
                .header("cookie", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"preferences":{"systemEnabled":true,"mentionEnabled":false,"replyEnabled":true,"followedPublisherPostEnabled":false,"moderationEnabled":false}}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::NO_CONTENT);

    let preferences = app
        .oneshot(Request::get("/notification-preferences").header("cookie", &cookie).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(preferences.status(), StatusCode::OK);
    assert_eq!(
        json(preferences).await,
        serde_json::json!({
            "systemEnabled": true,
            "mentionEnabled": false,
            "replyEnabled": true,
            "followedPublisherPostEnabled": false,
            "moderationEnabled": false
        })
    );
}
