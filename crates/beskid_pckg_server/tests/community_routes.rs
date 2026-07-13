#[path = "../src/community_routes.rs"]
mod community_routes;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use beskid_pckg_auth::{AuthHubIdentity, issue_pckg_session};
use beskid_pckg_community::{Board, BoardId};
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
    assert_eq!(
        json(response).await,
        serde_json::json!({"message": "authentication required"})
    );
}

#[tokio::test]
async fn authenticated_session_can_create_a_board_post() {
    let state = community_routes::CommunityState::with_session_secret("test-session-secret");
    state
        .service()
        .lock()
        .unwrap()
        .add_board(Board::new(BoardId::new("general").unwrap(), "General"));

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
