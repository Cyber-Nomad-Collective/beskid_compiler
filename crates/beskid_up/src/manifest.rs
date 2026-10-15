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
    commit: String,
    bundles: Vec<Bundle>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    pub target: String,
    pub url: String,
    pub sha256: String,
}

#[derive(Debug)]
pub struct ReleaseManifest {
    pub version: Version,
    pub commit: String,
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
        if raw.commit.len() != 40
            || !raw.commit.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(UpError::InvalidManifest("commit must be a 40-character lowercase source SHA".into()));
        }
        let mut targets = std::collections::HashSet::new();
        for bundle in &raw.bundles {
            if !targets.insert(&bundle.target) {
                return Err(UpError::InvalidManifest("duplicate bundle target".into()));
            }
            beskid_abi::abi_v5::TargetMetadata::for_triple(&bundle.target)
                .map_err(|_| UpError::UnsupportedTarget(bundle.target.clone()))?;
            let immutable_origin = format!("{RELEASE_ORIGIN}releases/download/cli-v{version}/");
            if !bundle.url.starts_with(&immutable_origin) || bundle.url.contains('?') || bundle.url.contains('#') {
                return Err(UpError::InvalidManifest(format!(
                    "bundle URL is outside the Beskid release origin: {}",
                    bundle.url
                )));
            }
            if bundle.sha256.len() != 64 || !bundle.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(UpError::InvalidManifest(format!("bundle checksum is not SHA-256: {}", bundle.target)));
            }
        }
        Ok(Self { version, commit: raw.commit, bundles: raw.bundles })
    }

    pub fn select_bundle(&self, target: &str) -> Result<&Bundle, UpError> {
        self.bundles
            .iter()
            .find(|bundle| bundle.target == target)
            .ok_or_else(|| UpError::UnsupportedTarget(target.to_owned()))
    }
}
