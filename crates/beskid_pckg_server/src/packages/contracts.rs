#[derive(serde::Deserialize)]
pub(crate) struct CommunityReviewRequest {
    pub(super) rating: i16,
    pub(super) comment: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CommunityReviewResponse {
    pub(super) id: String,
    pub(super) author: String,
    pub(super) rating: i16,
    pub(super) comment: String,
    pub(super) created_at_utc: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PublisherResponse {
    pub(super) subject: String,
    pub(super) display_name: String,
    pub(super) bio: String,
    pub(super) social_links: Vec<String>,
    pub(super) is_publisher_verified: bool,
    pub(super) package_count: usize,
}

#[derive(serde::Deserialize)]
pub(crate) struct PackageVersionPath {
    pub(super) name: String,
    pub(super) version: String,
}

#[derive(Debug, Default, serde::Deserialize)]
pub(crate) struct ListQuery {
    pub(super) q: Option<String>,
    pub(super) owner: Option<String>,
    pub(super) limit: Option<i64>,
    pub(super) offset: Option<i64>,
    pub(super) page: Option<i64>,
}

impl ListQuery {
    pub(super) fn limit(&self) -> i64 {
        self.limit.unwrap_or(100).clamp(1, 200)
    }

    pub(super) fn offset(&self) -> i64 {
        self.offset.unwrap_or_else(|| self.page.unwrap_or(0).max(0).saturating_mul(self.limit())).max(0)
    }

    pub(super) fn query(&self) -> Option<String> {
        self.q.as_deref().map(str::trim).filter(|value| !value.is_empty()).map(str::to_ascii_lowercase)
    }

    pub(super) fn requests_current_owner(&self) -> bool {
        self.owner.as_deref() == Some("me")
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DeletePackageResponse {
    pub(super) success: bool,
    pub(super) message: String,
}
