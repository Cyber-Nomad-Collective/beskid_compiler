use beskid_pckg_contract::{
    PackageContractFixture, PackageHealthSnapshotResponse, PackageSearchResponse,
    PackageSummaryResponse, PackageVersionLifecycleResponse, PackageVersionSummaryResponse,
    PublishPackageVersionRequest, UpsertPackageRequest,
};

fn health() -> PackageHealthSnapshotResponse {
    PackageHealthSnapshotResponse {
        state: "healthy".to_owned(),
        sub_state: "current".to_owned(),
        score: 0.95,
        update_rate_state: "healthy".to_owned(),
        update_rate_sub_state: "current".to_owned(),
        update_rate_normalized: 1.0,
        update_rate_weight: 0.4,
        downloads_state: "healthy".to_owned(),
        downloads_sub_state: "active".to_owned(),
        downloads_normalized: 0.8,
        downloads_weight: 0.3,
        reviews_state: "healthy".to_owned(),
        reviews_sub_state: "reviewed".to_owned(),
        reviews_normalized: 0.9,
        reviews_weight: 0.3,
    }
}

fn package(name: &str, is_public: bool) -> PackageSummaryResponse {
    PackageSummaryResponse {
        id: format!("{name}-id"),
        name: name.to_owned(),
        description: "A package contract fixture.".to_owned(),
        category: "General".to_owned(),
        repository_url: Some("https://example.test/repository".to_owned()),
        website_url: Some("https://example.test".to_owned()),
        tags: vec!["fixtures".to_owned()],
        is_public,
        total_downloads: 42,
        updated_at_utc: "2026-07-13T12:00:00Z".to_owned(),
        pending_reviews_count: 0,
        average_rating: 4.5,
        icon_url: Some("https://example.test/icon.svg".to_owned()),
        owner_user_id: "owner-1".to_owned(),
        owner_display_name: "Fixture Owner".to_owned(),
        owner_is_publisher_verified: true,
    }
}

fn version(
    package_name: &str,
    value: &str,
    published_at_utc: &str,
    is_yanked: bool,
) -> PackageVersionSummaryResponse {
    PackageVersionSummaryResponse {
        id: format!("{package_name}-{value}"),
        package_id: format!("{package_name}-id"),
        package_name: package_name.to_owned(),
        version: value.to_owned(),
        is_yanked,
        checksum_sha256: format!("sha256-{value}"),
        size_bytes: 512,
        published_at_utc: published_at_utc.to_owned(),
        yanked_at_utc: is_yanked.then(|| "2026-07-13T13:00:00Z".to_owned()),
        has_readme: true,
        configuration_json: Some(r#"{"profile":"release"}"#.to_owned()),
        overrides_json: Some(r#"{"strict":true}"#.to_owned()),
    }
}

#[test]
fn list_and_search_contracts_preserve_the_full_summary_wire_shape() {
    let summary = package("Public.Demo", true);
    let search = PackageSearchResponse {
        package: summary.clone(),
        review_count: 3,
        health: health(),
    };

    assert_eq!(
        serde_json::to_value(vec![summary]).unwrap(),
        serde_json::json!([{
            "id": "Public.Demo-id", "name": "Public.Demo", "description": "A package contract fixture.",
            "category": "General", "repositoryUrl": "https://example.test/repository",
            "websiteUrl": "https://example.test", "tags": ["fixtures"], "isPublic": true,
            "totalDownloads": 42, "updatedAtUtc": "2026-07-13T12:00:00Z",
            "pendingReviewsCount": 0, "averageRating": 4.5, "iconUrl": "https://example.test/icon.svg",
            "ownerUserId": "owner-1", "ownerDisplayName": "Fixture Owner", "ownerIsPublisherVerified": true
        }])
    );
    assert_eq!(serde_json::to_value(search).unwrap()["reviewCount"], 3);
}

#[test]
fn detail_and_version_contracts_preserve_artifact_metadata() {
    let fixture = PackageContractFixture::new(
        package("Details.Demo", true),
        vec![version(
            "Details.Demo",
            "1.0.0",
            "2026-07-13T12:00:00Z",
            false,
        )],
        health(),
    );

    let details = fixture.detail_for(None).expect("public package is visible");
    let json = serde_json::to_value(details).unwrap();
    assert_eq!(json["versions"][0]["hasReadme"], true);
    assert_eq!(
        json["versions"][0]["configuration"],
        serde_json::json!({"profile":"release"})
    );
    assert_eq!(
        json["versions"][0]["overrides"],
        serde_json::json!({"strict":true})
    );
    assert_eq!(json["latestVersion"], "1.0.0");
}

#[test]
fn private_package_detail_is_intentionally_not_found_for_non_owners() {
    let fixture = PackageContractFixture::new(package("Private.Demo", false), vec![], health());

    assert!(fixture.detail_for(None).is_none());
    assert!(fixture.detail_for(Some("other-user")).is_none());
    assert!(fixture.detail_for(Some("owner-1")).is_some());
}

#[test]
fn latest_download_resolves_to_the_most_recent_non_yanked_version() {
    let fixture = PackageContractFixture::new(
        package("Latest.Demo", true),
        vec![
            version("Latest.Demo", "1.0.0", "2026-07-13T10:00:00Z", false),
            version("Latest.Demo", "2.0.0", "2026-07-13T11:00:00Z", true),
            version("Latest.Demo", "1.1.0", "2026-07-13T12:00:00Z", false),
        ],
        health(),
    );

    let download = fixture
        .download_for(None, "latest")
        .expect("latest active download");
    assert_eq!(download.version.version, "1.1.0");
    assert_eq!(download.content_type, "application/zip");
    assert_eq!(
        download.content_disposition,
        "attachment; filename=Latest.Demo-1.1.0.bpk"
    );
    assert!(fixture.download_for(None, "2.0.0").is_none());
}

#[test]
fn yank_and_unyank_lifecycle_responses_keep_the_version_payload() {
    let version = version("Yank.Demo", "1.0.0", "2026-07-13T12:00:00Z", true);
    let response = PackageVersionLifecycleResponse {
        success: true,
        message: "version yanked".to_owned(),
        version: Some(version),
    };

    let json = serde_json::to_value(response).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["version"]["isYanked"], true);
    assert_eq!(json["version"]["yankedAtUtc"], "2026-07-13T13:00:00Z");
}

#[test]
fn checksum_matched_publish_is_an_idempotent_success() {
    let existing = version("Idempotent.Demo", "1.0.0", "2026-07-13T12:00:00Z", false);
    let request = PublishPackageVersionRequest {
        version: Some("1.0.0".to_owned()),
        version_bump: None,
        checksum_sha256: Some(existing.checksum_sha256.clone()),
    };

    assert!(request.is_idempotent_against(&existing));
    assert!(
        !PublishPackageVersionRequest {
            checksum_sha256: Some("different-checksum".to_owned()),
            ..request
        }
        .is_idempotent_against(&existing)
    );
}

#[test]
fn upsert_request_uses_the_legacy_camel_case_names() {
    let request = UpsertPackageRequest {
        name: "Managed.Demo".to_owned(),
        description: Some("updated".to_owned()),
        category: Some("General".to_owned()),
        repository_url: None,
        website_url: None,
        tags: Some(vec!["fixtures".to_owned()]),
        is_public: true,
        submit_for_review: false,
        review_reason: None,
        icon_url: None,
    };

    assert_eq!(
        serde_json::to_value(request).unwrap(),
        serde_json::json!({
            "name": "Managed.Demo", "description": "updated", "category": "General",
            "repositoryUrl": null, "websiteUrl": null, "tags": ["fixtures"], "isPublic": true,
            "submitForReview": false, "reviewReason": null, "iconUrl": null
        })
    );
}
