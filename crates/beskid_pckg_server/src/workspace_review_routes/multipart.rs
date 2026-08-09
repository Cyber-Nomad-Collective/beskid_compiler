use axum::http::header;

use super::contracts::{MAX_WORKSPACE_BYTES, VersionBump};

pub(super) async fn multipart_artifact(
    request: axum::extract::Request,
) -> Result<(Vec<u8>, VersionBump), &'static str> {
    let content_type = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .ok_or("Expected multipart form payload.")?;
    let boundary = multer::parse_boundary(content_type).map_err(|_| "Expected multipart form payload.")?;
    let constraints = multer::Constraints::new().allowed_fields(vec!["artifact", "versionBump"]).size_limit(
        multer::SizeLimit::new()
            .whole_stream(MAX_WORKSPACE_BYTES as u64)
            .for_field("artifact", MAX_WORKSPACE_BYTES as u64),
    );
    let mut form = multer::Multipart::with_constraints(request.into_body().into_data_stream(), boundary, constraints);
    let mut artifact = None;
    let mut version_bump = VersionBump::Patch;
    while let Some(field) = form.next_field().await.map_err(|_| "Invalid workspace multipart payload.")? {
        let name = field.name().unwrap_or_default().to_owned();
        let bytes = field.bytes().await.map_err(|_| "Invalid workspace multipart payload.")?;
        if name == "artifact" && artifact.is_none() {
            artifact = Some(bytes.to_vec());
        } else if name == "versionBump" {
            version_bump =
                match std::str::from_utf8(&bytes).ok().map(str::trim).unwrap_or_default().to_ascii_lowercase().as_str()
                {
                    "" | "patch" => VersionBump::Patch,
                    "minor" => VersionBump::Minor,
                    "major" => VersionBump::Major,
                    _ => return Err("versionBump must be patch, minor, or major."),
                };
        } else if name != "versionBump" {
            return Err("Invalid workspace multipart payload.");
        }
    }
    artifact
        .filter(|bytes| !bytes.is_empty())
        .map(|artifact| (artifact, version_bump))
        .ok_or("Artifact file is required.")
}
