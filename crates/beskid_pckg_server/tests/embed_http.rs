use axum::body::Body;
use axum::http::{Request, StatusCode};
use beskid_pckg_server::{PckgServerConfig, router};
use http_body_util::BodyExt;
use tower::ServiceExt;

fn config() -> PckgServerConfig {
    PckgServerConfig::default().with_authelia_auth()
}

async fn text(response: axum::response::Response) -> String {
    String::from_utf8(response.into_body().collect().await.expect("body is readable").to_bytes().to_vec())
        .expect("response is UTF-8")
}

#[tokio::test]
async fn public_embed_card_and_badge_match_legacy_content_contract_without_private_leakage() {
    let app = router(config());

    for (name, is_public) in [("Public.Embed", true), ("Private.Embed", false), ("@pckg/demo-lib", true)] {
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/packages")
                    .header("content-type", "application/json")
                    .header("remote-user", "owner")
                    .body(Body::from(
                        serde_json::json!({"name": name, "isPublic": is_public, "submitForReview": false}).to_string(),
                    ))
                    .expect("request builds"),
            )
            .await
            .expect("route responds");
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    let published = app
        .clone()
        .oneshot(
            Request::post("/api/packages/Public.Embed/versions")
                .header("content-type", "application/json")
                .header("remote-user", "owner")
                .body(Body::from(
                    r#"{"version":"1.2.3","checksumSha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#,
                ))
                .expect("request builds"),
        )
        .await
        .expect("route responds");
    assert_eq!(published.status(), StatusCode::CREATED);

    let badge = app
        .clone()
        .oneshot(Request::get("/api/embed/badge.svg?package=Public.Embed").body(Body::empty()).expect("request builds"))
        .await
        .expect("route responds");
    assert_eq!(badge.status(), StatusCode::OK);
    assert_eq!(badge.headers().get("content-type").expect("content type"), "image/svg+xml; charset=utf-8");
    assert_eq!(badge.headers().get("cache-control").expect("cache control"), "public, max-age=120");
    assert!(text(badge).await.contains("Public.Embed · 1.2.3"));

    let card = app
        .clone()
        .oneshot(
            Request::get("/api/embed/card?package=Public.Embed")
                .header("host", "registry.example")
                .header("x-forwarded-proto", "https")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("route responds");
    assert_eq!(card.status(), StatusCode::OK);
    assert_eq!(card.headers().get("content-type").expect("content type"), "text/html; charset=utf-8");
    assert_eq!(card.headers().get("content-security-policy").expect("CSP"), "frame-ancestors *");
    assert_eq!(card.headers().get("cache-control").expect("cache control"), "public, max-age=120");
    let card_body = text(card).await;
    assert!(card_body.contains("Public.Embed"));
    assert!(card_body.contains("color-scheme:light dark"));
    assert!(card_body.contains("https://registry.example/api/embed/badge.svg?package=Public.Embed"));

    let private_card = app
        .clone()
        .oneshot(Request::get("/api/embed/card?package=Private.Embed").body(Body::empty()).expect("request builds"))
        .await
        .expect("route responds");
    assert_eq!(private_card.status(), StatusCode::NOT_FOUND);
    assert!(!text(private_card).await.contains("Private.Embed"));

    let private_badge = app
        .clone()
        .oneshot(
            Request::get("/api/embed/badge.svg?package=Private.Embed").body(Body::empty()).expect("request builds"),
        )
        .await
        .expect("route responds");
    assert_eq!(private_badge.status(), StatusCode::OK);
    let private_badge_body = text(private_badge).await;
    assert!(private_badge_body.contains("not found"));
    assert!(!private_badge_body.contains("Private.Embed"));

    let scoped_card = app
        .oneshot(
            Request::get("/api/embed/card?package=%40pckg%2Fdemo-lib")
                .header("host", "registry.example")
                .header("x-forwarded-proto", "https")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("route responds");
    assert_eq!(scoped_card.status(), StatusCode::OK);
    let scoped_card_body = text(scoped_card).await;
    assert!(scoped_card_body.contains("%40pckg%2Fdemo-lib"));
}
