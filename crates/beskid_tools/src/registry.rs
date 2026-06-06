//! Shared pckg registry client helpers for CLI commands.

use anyhow::{Context, Result, anyhow};
use beskid_pckg::models::PackageVersionSummaryResponse;
use beskid_pckg::{PckgClient, PckgClientConfig, PckgError};
use semver::Version;

/// Connection options for the package registry HTTP client.
#[derive(Debug, Clone, Default)]
pub struct RegistryConnectConfig {
    pub registry_url: String,
    pub bearer_token: Option<String>,
    pub api_key: Option<String>,
}

impl RegistryConnectConfig {
    pub fn new(registry_url: impl Into<String>) -> Self {
        Self {
            registry_url: registry_url.into(),
            bearer_token: None,
            api_key: None,
        }
    }

    pub fn with_bearer_token(mut self, token: impl Into<String>) -> Self {
        self.bearer_token = Some(token.into());
        self
    }

    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }
}

/// Build a [`PckgClient`] from [`RegistryConnectConfig`].
pub fn build_pckg_client(config: &RegistryConnectConfig) -> Result<PckgClient> {
    let mut client_config = PckgClientConfig::new(&config.registry_url)?;
    client_config = match (&config.bearer_token, &config.api_key) {
        (Some(token), _) => client_config.with_bearer_token(token),
        (None, Some(key)) => client_config.with_publisher_api_key(key),
        (None, None) => client_config,
    };
    PckgClient::new(client_config).map_err(pckg_to_anyhow)
}

/// Multi-thread Tokio runtime for blocking on async pckg client calls.
pub fn tokio_runtime() -> Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("tokio runtime")
}

/// Parse `package@version` or bare package id.
pub fn parse_package_selector(selector: &str) -> Result<(String, Option<String>)> {
    if let Some((id, ver)) = selector.split_once('@') {
        Ok((id.to_string(), Some(ver.to_string())))
    } else {
        Ok((selector.to_string(), None))
    }
}

/// Pick an explicit version or the latest non-yanked release.
pub fn pick_version(
    versions: &[PackageVersionSummaryResponse],
    explicit: Option<&str>,
) -> Result<PackageVersionSummaryResponse> {
    if let Some(v) = explicit {
        return versions
            .iter()
            .find(|entry| entry.version == v)
            .cloned()
            .ok_or_else(|| anyhow!("version `{v}` not found"));
    }
    latest_non_yanked(versions).ok_or_else(|| anyhow!("no published versions found"))
}

/// Highest semver among non-yanked versions.
pub fn latest_non_yanked(
    versions: &[PackageVersionSummaryResponse],
) -> Option<PackageVersionSummaryResponse> {
    versions
        .iter()
        .filter(|v| !v.is_yanked)
        .filter_map(|v| Version::parse(&v.version).ok().map(|parsed| (parsed, v)))
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, v)| (*v).clone())
}

/// Map [`PckgError`] into [`anyhow::Error`].
pub fn pckg_to_anyhow(err: PckgError) -> anyhow::Error {
    anyhow!("{err}")
}

/// Heuristic: registry connectivity failures for user-facing messages.
pub fn is_network_error(err: &anyhow::Error) -> bool {
    err.to_string().contains("registry") || err.to_string().contains("connect")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_package_selector() {
        let (id, ver) = parse_package_selector("beskid.templates.console@1.0.0").unwrap();
        assert_eq!(id, "beskid.templates.console");
        assert_eq!(ver.as_deref(), Some("1.0.0"));
    }
}
