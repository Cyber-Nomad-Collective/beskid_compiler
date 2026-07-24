use semver::Version;
use serde::Deserialize;
use thiserror::Error;

const RELEASE_ORIGIN: &str = "https://github.com/Cyber-Nomad-Collective/beskid_compiler/";

#[derive(Debug, Error)]
pub enum UpError {
    #[error("invalid release manifest: {0}")]
    InvalidManifest(String),
    #[error("no bundle exists for target {0}")]
    UnsupportedTarget(String),
}

#[derive(Debug, Deserialize)]
struct RawManifest {
    schema: u32,
    version: String,
    bundles: Vec<Bundle>,
}

#[derive(Debug, Deserialize)]
pub struct Bundle {
    pub target: String,
    pub url: String,
    pub sha256: String,
}

#[derive(Debug)]
pub struct ReleaseManifest {
    pub version: Version,
    bundles: Vec<Bundle>,
}

impl ReleaseManifest {
    pub fn from_json(input: &str) -> Result<Self, UpError> {
        let raw: RawManifest =
            serde_json::from_str(input).map_err(|error| UpError::InvalidManifest(error.to_string()))?;
        if raw.schema != 1 {
            return Err(UpError::InvalidManifest(format!("unsupported schema {}", raw.schema)));
        }
        let version = Version::parse(&raw.version)
            .map_err(|error| UpError::InvalidManifest(format!("invalid version: {error}")))?;
        if raw.bundles.is_empty() {
            return Err(UpError::InvalidManifest("bundles must not be empty".into()));
        }
        for bundle in &raw.bundles {
            if !bundle.url.starts_with(RELEASE_ORIGIN) {
                return Err(UpError::InvalidManifest(format!(
                    "bundle URL is outside the Beskid release origin: {}",
                    bundle.url
                )));
            }
            if bundle.sha256.len() != 64 || !bundle.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(UpError::InvalidManifest(format!("bundle checksum is not SHA-256: {}", bundle.target)));
            }
        }
        Ok(Self { version, bundles: raw.bundles })
    }

    pub fn select_bundle(&self, target: &str) -> Result<&Bundle, UpError> {
        self.bundles
            .iter()
            .find(|bundle| bundle.target == target)
            .ok_or_else(|| UpError::UnsupportedTarget(target.to_owned()))
    }
}
