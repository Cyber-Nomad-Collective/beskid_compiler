//! pckg registry fetch helpers for the package browser pane.

use anyhow::Result;
use beskid_pckg::models::{PackageDetailsResponse, PackageSummaryResponse};

use crate::registry::{RegistryConnectConfig, build_pckg_client, pckg_to_anyhow, tokio_runtime};

pub fn fetch_packages(config: &RegistryConnectConfig) -> Result<Vec<PackageSummaryResponse>> {
    let client = build_pckg_client(config)?;
    let runtime = tokio_runtime()?;
    runtime.block_on(client.list_packages()).map_err(pckg_to_anyhow)
}

pub fn search_packages(config: &RegistryConnectConfig, query: &str) -> Result<Vec<PackageSummaryResponse>> {
    let client = build_pckg_client(config)?;
    let runtime = tokio_runtime()?;
    let hits = runtime.block_on(client.search_packages(query)).map_err(pckg_to_anyhow)?;
    Ok(hits.into_iter().map(|hit| hit.package).collect())
}

pub fn fetch_package_details(config: &RegistryConnectConfig, package_id: &str) -> Result<PackageDetailsResponse> {
    let client = build_pckg_client(config)?;
    let runtime = tokio_runtime()?;
    runtime.block_on(client.get_package_details(package_id)).map_err(pckg_to_anyhow)
}
