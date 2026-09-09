use std::io::Read;
use std::path::Path;

use crate::progress::UploadProgress;
use reqwest::Method;
use reqwest::multipart;
use tokio::fs::File;
use tokio_util::io::ReaderStream;

use crate::client::PckgClient;
use crate::error::PckgError;
use crate::models::{
    PackageDetailsResponse, PackageFileListResponse, PackageReviewResponse, PackageSearchResponse,
    PackageSummaryResponse, PackageVersionLifecycleResponse, PackageVersionSummaryResponse, ReviewActionRequest,
    ReviewActionResponse, UpsertPackageRequest, UpsertPackageResponse,
};

fn ensure_upsert_success(
    response: UpsertPackageResponse,
    body_hint: Option<String>,
) -> Result<UpsertPackageResponse, PckgError> {
    if response.success { Ok(response) } else { Err(PckgError::logical_failure(response.message.clone(), body_hint)) }
}

fn ensure_review_success(
    response: ReviewActionResponse,
    body_hint: Option<String>,
) -> Result<ReviewActionResponse, PckgError> {
    if response.success { Ok(response) } else { Err(PckgError::logical_failure(response.message.clone(), body_hint)) }
}

fn ensure_lifecycle_success(
    response: PackageVersionLifecycleResponse,
    body_hint: Option<String>,
) -> Result<PackageVersionLifecycleResponse, PckgError> {
    if response.success { Ok(response) } else { Err(PckgError::logical_failure(response.message.clone(), body_hint)) }
}

impl PckgClient {
    pub async fn list_packages(&self) -> Result<Vec<PackageSummaryResponse>, PckgError> {
        self.send_no_body(Method::GET, "/api/packages", false).await
    }

    pub async fn upsert_package(&self, request: &UpsertPackageRequest) -> Result<UpsertPackageResponse, PckgError> {
        let response: UpsertPackageResponse = self.send_with_body(Method::POST, "/api/packages", request, true).await?;
        ensure_upsert_success(response, None)
    }

    pub async fn list_review_queue(&self) -> Result<Vec<PackageReviewResponse>, PckgError> {
        self.send_no_body(Method::GET, "/api/packages/reviews", true).await
    }

    pub async fn review_action(&self, request: &ReviewActionRequest) -> Result<ReviewActionResponse, PckgError> {
        let path = format!("/api/packages/reviews/{}/actions", request.review_id);
        let response: ReviewActionResponse = self.send_with_body(Method::POST, &path, request, true).await?;
        ensure_review_success(response, None)
    }

    pub async fn list_package_versions(
        &self,
        package_name: &str,
    ) -> Result<Vec<PackageVersionSummaryResponse>, PckgError> {
        let path = format!("/api/packages/{}/versions", package_name);
        self.send_no_body(Method::GET, &path, false).await
    }

    /// Publish a `.bpk` from a local file through the canonical version endpoint.
    ///
    /// The immutable version is read from the artifact-root `package.json`, while the artifact is
    /// streamed without a full-file buffer. The checksum is always present on the wire: callers
    /// may provide a precomputed value or let this method compute it from the file.
    /// `upload_progress`: when set, reports byte progress on stderr during the HTTP upload.
    pub async fn publish_package_version(
        &self,
        package_name: &str,
        artifact_path: &Path,
        artifact_name: &str,
        checksum_sha256: Option<&str>,
        upload_progress: Option<&UploadProgress>,
    ) -> Result<PackageVersionSummaryResponse, PckgError> {
        if self.config().auth.is_none() {
            return Err(PckgError::MissingAuthToken);
        }

        let version = artifact_version(artifact_path, package_name)?;
        let path = format!("/api/packages/{}/versions", package_name);

        let mut file = File::open(artifact_path).await.map_err(PckgError::Io)?;
        let len = file.metadata().await.map_err(PckgError::Io)?.len();
        let checksum_sha256 = match checksum_sha256.map(str::trim).filter(|value| !value.is_empty()) {
            Some(checksum) => checksum.to_owned(),
            None => {
                use sha2::{Digest, Sha256};
                use tokio::io::{AsyncReadExt, AsyncSeekExt};

                let mut digest = Sha256::new();
                let mut buffer = [0_u8; 64 * 1024];
                loop {
                    let read = file.read(&mut buffer).await.map_err(PckgError::Io)?;
                    if read == 0 {
                        break;
                    }
                    digest.update(&buffer[..read]);
                }
                file.seek(std::io::SeekFrom::Start(0)).await.map_err(PckgError::Io)?;
                format!("{:x}", digest.finalize())
            }
        };

        let tracked_file: std::pin::Pin<Box<dyn tokio::io::AsyncRead + Send>> = if let Some(progress) = upload_progress
        {
            Box::pin(progress.wrap_async_read(file))
        } else {
            Box::pin(file)
        };

        let stream = ReaderStream::new(tracked_file);
        let body = reqwest::Body::wrap_stream(stream);
        let part = multipart::Part::stream_with_length(body, len)
            .file_name(artifact_name.to_string())
            .mime_str("application/zip")
            .map_err(PckgError::Transport)?;

        let form = multipart::Form::new()
            .text("version", version)
            .text("checksumSha256", checksum_sha256)
            .part("artifact", part);

        self.send_multipart(Method::POST, &path, form, true).await
    }

    pub async fn download_package_version(&self, package_name: &str, version: &str) -> Result<Vec<u8>, PckgError> {
        let path = format!("/api/packages/{}/versions/{}/download", package_name, version);
        let response = self.send_streaming(Method::GET, &path, false).await?;
        let bytes = response.bytes().await.map_err(PckgError::Transport)?;
        Ok(bytes.to_vec())
    }

    /// List every file entry inside a package version (sparse file-tree
    /// manifest).  Does NOT download the artifact — callers use the
    /// returned [`PackageFileListResponse`] to pick individual files
    /// for streaming via [`stream_package_file`].
    pub async fn list_package_files(
        &self,
        package_name: &str,
        version: &str,
    ) -> Result<PackageFileListResponse, PckgError> {
        let path = format!("/api/packages/{}/versions/{}/files", package_name, version);
        self.send_no_body(Method::GET, &path, false).await
    }

    /// Stream a single file from a package version sparsely, without
    /// downloading the entire `.bpk` artifact.
    ///
    /// Returns a raw [`reqwest::Response`] whose body can be consumed
    /// incrementally via `.bytes_stream()`.  The caller MUST drop the
    /// response when done to release the connection-pool slot.
    pub async fn stream_package_file(
        &self,
        package_name: &str,
        version: &str,
        file_path: &str,
    ) -> Result<reqwest::Response, PckgError> {
        let path =
            format!("/api/packages/{}/versions/{}/files/{}", package_name, version, file_path.trim_start_matches('/'));
        self.send_streaming(Method::GET, &path, false).await
    }

    pub async fn get_package_details(&self, id_or_name: &str) -> Result<PackageDetailsResponse, PckgError> {
        let path = format!("/api/packages/{id_or_name}");
        self.send_no_body(Method::GET, &path, false).await
    }

    pub async fn search_packages(&self, query: &str) -> Result<Vec<PackageSearchResponse>, PckgError> {
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
        let response: PackageVersionLifecycleResponse = self.send_no_body(Method::POST, &path, true).await?;
        ensure_lifecycle_success(response, None)
    }

    pub async fn unyank_package_version(
        &self,
        package_name: &str,
        version: &str,
    ) -> Result<PackageVersionLifecycleResponse, PckgError> {
        let path = format!("/api/packages/{}/versions/{}/unyank", package_name, version);
        let response: PackageVersionLifecycleResponse = self.send_no_body(Method::POST, &path, true).await?;
        ensure_lifecycle_success(response, None)
    }
}

fn artifact_version(path: &Path, expected_package_name: &str) -> Result<String, PckgError> {
    const MAX_MANIFEST_BYTES: u64 = 64 * 1024;

    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| artifact_contract_error(format!("invalid package artifact ZIP: {error}")))?;
    let mut manifest = archive
        .by_name("package.json")
        .map_err(|_| artifact_contract_error("package artifact is missing root package.json"))?;
    if manifest.size() > MAX_MANIFEST_BYTES {
        return Err(artifact_contract_error("package artifact manifest exceeds 64 KiB"));
    }
    let mut json = String::new();
    manifest
        .read_to_string(&mut json)
        .map_err(|error| artifact_contract_error(format!("package artifact manifest is unreadable: {error}")))?;
    let value: serde_json::Value = serde_json::from_str(&json)
        .map_err(|_| artifact_contract_error("package artifact manifest is not valid JSON"))?;
    let package = value.get("id").and_then(serde_json::Value::as_str).unwrap_or_default();
    if !package.eq_ignore_ascii_case(expected_package_name.trim()) {
        return Err(artifact_contract_error("package artifact id does not match the requested package"));
    }
    let version = value.get("version").and_then(serde_json::Value::as_str).unwrap_or_default();
    semver::Version::parse(version)
        .map_err(|_| artifact_contract_error("package artifact version is not valid semantic version"))?;
    Ok(version.to_owned())
}

fn artifact_contract_error(message: impl Into<String>) -> PckgError {
    PckgError::logical_failure(message, None)
}
