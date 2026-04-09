use reqwest::Method;
use reqwest::multipart;

use crate::client::PckgClient;
use crate::error::PckgError;
use crate::models::{
    PackageDetailsResponse, PackageReviewResponse, PackageSearchResponse, PackageSummaryResponse,
    PackageVersionLifecycleResponse, PackageVersionSummaryResponse, PublishPackageVersionResponse,
    ReviewActionRequest, ReviewActionResponse, UpsertPackageRequest, UpsertPackageResponse,
};

impl PckgClient {
    pub async fn list_packages(&self) -> Result<Vec<PackageSummaryResponse>, PckgError> {
        self.send_no_body(Method::GET, "/api/packages", false).await
    }

    pub async fn upsert_package(
        &self,
        request: &UpsertPackageRequest,
    ) -> Result<UpsertPackageResponse, PckgError> {
        self.send_with_body(Method::POST, "/api/packages", request, true)
            .await
    }

    pub async fn list_review_queue(&self) -> Result<Vec<PackageReviewResponse>, PckgError> {
        self.send_no_body(Method::GET, "/api/packages/reviews", true)
            .await
    }

    pub async fn review_action(
        &self,
        request: &ReviewActionRequest,
    ) -> Result<ReviewActionResponse, PckgError> {
        let path = format!("/api/packages/reviews/{}/actions", request.review_id);
        self.send_with_body(Method::POST, &path, request, true)
            .await
    }

    pub async fn list_package_versions(
        &self,
        package_name: &str,
    ) -> Result<Vec<PackageVersionSummaryResponse>, PckgError> {
        let path = format!("/api/packages/{}/versions", package_name);
        self.send_no_body(Method::GET, &path, false).await
    }

    pub async fn publish_package_version(
        &self,
        package_name: &str,
        version: &str,
        artifact_name: &str,
        artifact_bytes: Vec<u8>,
        manifest_json: Option<&str>,
        checksum_sha256: Option<&str>,
    ) -> Result<PublishPackageVersionResponse, PckgError> {
        let path = format!("/api/packages/{}/publish", package_name);

        let part = multipart::Part::bytes(artifact_bytes)
            .file_name(artifact_name.to_string())
            .mime_str("application/zip")
            .map_err(PckgError::Transport)?;

        let mut form = multipart::Form::new()
            .text("version", version.to_string())
            .part("artifact", part);

        if let Some(manifest_json) = manifest_json {
            form = form.text("manifestJson", manifest_json.to_string());
        }

        if let Some(checksum_sha256) = checksum_sha256 {
            form = form.text("checksumSha256", checksum_sha256.to_string());
        }

        self.send_multipart(Method::POST, &path, form, true).await
    }

    pub async fn download_package_version(
        &self,
        package_name: &str,
        version: &str,
    ) -> Result<Vec<u8>, PckgError> {
        let path = format!(
            "/api/packages/{}/versions/{}/download",
            package_name, version
        );
        self.send_no_body_bytes(Method::GET, &path, false).await
    }

    pub async fn get_package_details(
        &self,
        id_or_name: &str,
    ) -> Result<PackageDetailsResponse, PckgError> {
        let path = format!("/api/packages/{id_or_name}");
        self.send_no_body(Method::GET, &path, false).await
    }

    pub async fn search_packages(
        &self,
        query: &str,
    ) -> Result<Vec<PackageSearchResponse>, PckgError> {
        let encoded: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
        let path = format!("/api/search?q={encoded}");
        self.send_no_body(Method::GET, &path, false).await
    }

    pub async fn yank_package_version(
        &self,
        package_name: &str,
        version: &str,
    ) -> Result<PackageVersionLifecycleResponse, PckgError> {
        let path = format!("/api/packages/{}/versions/{}/yank", package_name, version);
        self.send_no_body(Method::POST, &path, true).await
    }

    pub async fn unyank_package_version(
        &self,
        package_name: &str,
        version: &str,
    ) -> Result<PackageVersionLifecycleResponse, PckgError> {
        let path = format!("/api/packages/{}/versions/{}/unyank", package_name, version);
        self.send_no_body(Method::POST, &path, true).await
    }
}
