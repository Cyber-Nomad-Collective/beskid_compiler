use std::path::Path;

use indicatif::ProgressBar;
use reqwest::Method;
use reqwest::multipart;
use tokio::fs::File;
use tokio_util::io::ReaderStream;

use crate::client::PckgClient;
use crate::error::PckgError;
use crate::models::{
    PackageDetailsResponse, PackageReviewResponse, PackageSearchResponse, PackageSummaryResponse,
    PackageVersionLifecycleResponse, PackageVersionSummaryResponse, PublishPackageVersionResponse,
    ReviewActionRequest, ReviewActionResponse, UpsertPackageRequest, UpsertPackageResponse,
};

fn ensure_publish_success(
    response: PublishPackageVersionResponse,
    body_hint: Option<String>,
) -> Result<PublishPackageVersionResponse, PckgError> {
    if response.success {
        Ok(response)
    } else {
        Err(PckgError::logical_failure(
            response.message.clone(),
            body_hint,
        ))
    }
}

fn ensure_upsert_success(
    response: UpsertPackageResponse,
    body_hint: Option<String>,
) -> Result<UpsertPackageResponse, PckgError> {
    if response.success {
        Ok(response)
    } else {
        Err(PckgError::logical_failure(
            response.message.clone(),
            body_hint,
        ))
    }
}

fn ensure_review_success(
    response: ReviewActionResponse,
    body_hint: Option<String>,
) -> Result<ReviewActionResponse, PckgError> {
    if response.success {
        Ok(response)
    } else {
        Err(PckgError::logical_failure(
            response.message.clone(),
            body_hint,
        ))
    }
}

fn ensure_lifecycle_success(
    response: PackageVersionLifecycleResponse,
    body_hint: Option<String>,
) -> Result<PackageVersionLifecycleResponse, PckgError> {
    if response.success {
        Ok(response)
    } else {
        Err(PckgError::logical_failure(
            response.message.clone(),
            body_hint,
        ))
    }
}

impl PckgClient {
    pub async fn list_packages(&self) -> Result<Vec<PackageSummaryResponse>, PckgError> {
        self.send_no_body(Method::GET, "/api/packages", false).await
    }

    pub async fn upsert_package(
        &self,
        request: &UpsertPackageRequest,
    ) -> Result<UpsertPackageResponse, PckgError> {
        let response: UpsertPackageResponse = self
            .send_with_body(Method::POST, "/api/packages", request, true)
            .await?;
        ensure_upsert_success(response, None)
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
        let response: ReviewActionResponse = self
            .send_with_body(Method::POST, &path, request, true)
            .await?;
        ensure_review_success(response, None)
    }

    pub async fn list_package_versions(
        &self,
        package_name: &str,
    ) -> Result<Vec<PackageVersionSummaryResponse>, PckgError> {
        let path = format!("/api/packages/{}/versions", package_name);
        self.send_no_body(Method::GET, &path, false).await
    }

    /// Publish a `.bpk` from a local file. Streams the artifact (no full-file buffer).
    /// `upload_progress`: when set, updates an indicatif bar during the HTTP upload.
    #[allow(clippy::too_many_arguments)]
    pub async fn publish_package_version(
        &self,
        package_name: &str,
        version: &str,
        artifact_path: &Path,
        artifact_name: &str,
        manifest_json: Option<&str>,
        checksum_sha256: Option<&str>,
        upload_progress: Option<&ProgressBar>,
    ) -> Result<PublishPackageVersionResponse, PckgError> {
        if self.config().auth.is_none() {
            return Err(PckgError::MissingAuthToken);
        }

        let path = format!("/api/packages/{}/publish", package_name);

        let file = File::open(artifact_path).await.map_err(PckgError::Io)?;
        let len = file.metadata().await.map_err(PckgError::Io)?.len();

        let tracked_file: std::pin::Pin<Box<dyn tokio::io::AsyncRead + Send>> = if let Some(pb) =
            upload_progress
        {
            pb.set_length(len);
            pb.set_style(
                    indicatif::ProgressStyle::with_template(
                        "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})",
                    )
                    .unwrap()
                    .progress_chars("#>-"),
                );
            Box::pin(pb.wrap_async_read(file))
        } else {
            Box::pin(file)
        };

        let stream = ReaderStream::new(tracked_file);
        let body = reqwest::Body::wrap_stream(stream);
        let part = multipart::Part::stream_with_length(body, len)
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

        let response: PublishPackageVersionResponse =
            self.send_multipart(Method::POST, &path, form, true).await?;
        if let Some(pb) = upload_progress {
            pb.finish_and_clear();
        }
        ensure_publish_success(response, None)
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
        let response: PackageVersionLifecycleResponse =
            self.send_no_body(Method::POST, &path, true).await?;
        ensure_lifecycle_success(response, None)
    }

    pub async fn unyank_package_version(
        &self,
        package_name: &str,
        version: &str,
    ) -> Result<PackageVersionLifecycleResponse, PckgError> {
        let path = format!("/api/packages/{}/versions/{}/unyank", package_name, version);
        let response: PackageVersionLifecycleResponse =
            self.send_no_body(Method::POST, &path, true).await?;
        ensure_lifecycle_success(response, None)
    }
}
