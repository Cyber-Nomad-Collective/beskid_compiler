use super::{
    ApiErrorResponse, ArtifactRecord, IntoResponse, Json, Package, PackageHealthSnapshotResponse,
    PackageSummaryResponse, PackageVersion, PackageVersionSummaryResponse, Response, StatusCode, select_download,
};

pub(super) fn package_storage_failure() -> Response {
    (StatusCode::SERVICE_UNAVAILABLE, Json(ApiErrorResponse::new("package storage unavailable"))).into_response()
}

pub(super) fn package_summary(package: &Package) -> PackageSummaryResponse {
    PackageSummaryResponse {
        id: package.id.clone(),
        name: package.name.clone(),
        description: String::new(),
        category: "General".to_owned(),
        repository_url: None,
        website_url: None,
        tags: Vec::new(),
        is_public: package.is_public,
        total_downloads: 0,
        updated_at_utc: timestamp(package.updated_at_unix_seconds),
        pending_reviews_count: 0,
        average_rating: 0.0,
        icon_url: None,
        owner_user_id: package.owner_subject.clone(),
        owner_display_name: package.owner_subject.clone(),
        owner_is_publisher_verified: false,
    }
}

pub(super) fn version_summary(package: &Package, version: &PackageVersion) -> PackageVersionSummaryResponse {
    PackageVersionSummaryResponse {
        id: version.id.clone(),
        package_id: package.id.clone(),
        package_name: package.name.clone(),
        version: version.version.clone(),
        is_yanked: version.is_yanked,
        checksum_sha256: version.checksum_sha256.clone(),
        size_bytes: version.size_bytes as i64,
        published_at_utc: timestamp(version.published_at_unix_seconds),
        yanked_at_utc: version.yanked_at_unix_seconds.map(timestamp),
        has_readme: false,
        configuration_json: None,
        overrides_json: None,
    }
}

pub(super) fn latest_non_yanked(versions: &[PackageVersion]) -> Option<&PackageVersion> {
    let records =
        versions.iter().map(|version| ArtifactRecord::new(&version.version, version.is_yanked)).collect::<Vec<_>>();
    let selected = select_download(&records, "latest")?;
    versions.iter().find(|version| version.version == selected.version)
}

pub(super) fn health() -> PackageHealthSnapshotResponse {
    PackageHealthSnapshotResponse {
        state: "unknown".to_owned(),
        sub_state: "unrated".to_owned(),
        score: 0.0,
        update_rate_state: "unknown".to_owned(),
        update_rate_sub_state: "unrated".to_owned(),
        update_rate_normalized: 0.0,
        update_rate_weight: 0.0,
        downloads_state: "unknown".to_owned(),
        downloads_sub_state: "unrated".to_owned(),
        downloads_normalized: 0.0,
        downloads_weight: 0.0,
        reviews_state: "unknown".to_owned(),
        reviews_sub_state: "unrated".to_owned(),
        reviews_normalized: 0.0,
        reviews_weight: 0.0,
    }
}

pub(super) fn package_not_found() -> axum::response::Response {
    (StatusCode::NOT_FOUND, Json(ApiErrorResponse::new("package not found"))).into_response()
}

pub(super) fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("clock is after Unix epoch").as_secs()
        as i64
}

pub(super) fn timestamp(seconds: i64) -> String {
    time::OffsetDateTime::from_unix_timestamp(seconds)
        .expect("repository timestamps are valid Unix seconds")
        .format(&time::format_description::well_known::Rfc3339)
        .expect("RFC3339 formatting succeeds")
}

pub(super) fn next_id(_kind: &str) -> String {
    uuid::Uuid::new_v4().to_string()
}
