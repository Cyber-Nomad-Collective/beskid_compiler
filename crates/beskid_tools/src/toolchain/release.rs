//! Download prebuilt CLI/LSP binaries from `beskid_compiler` GitHub releases.

use anyhow::{Context, Result, bail};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

pub const GITHUB_REPO: &str = "Cyber-Nomad-Collective/beskid_compiler";

#[derive(Debug, Clone, Copy)]
pub struct PlatformAsset {
    pub release_asset: &'static str,
    pub install_file_name: &'static str,
}

pub fn home_beskid_bin_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .context("HOME or USERPROFILE is not set")?;
    Ok(PathBuf::from(home).join(".beskid").join("bin"))
}

pub fn managed_lsp_path() -> Result<PathBuf> {
    let name = if cfg!(windows) { "beskid_lsp.exe" } else { "beskid_lsp" };
    Ok(home_beskid_bin_dir()?.join(name))
}

pub fn resolve_lsp_platform_asset() -> Option<PlatformAsset> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => {
            Some(PlatformAsset { release_asset: "beskid_lsp-linux-amd64", install_file_name: "beskid_lsp" })
        }
        ("macos", "aarch64") => {
            Some(PlatformAsset { release_asset: "beskid_lsp-darwin-arm64", install_file_name: "beskid_lsp" })
        }
        ("windows", "x86_64") => {
            Some(PlatformAsset { release_asset: "beskid_lsp-windows-amd64.exe", install_file_name: "beskid_lsp.exe" })
        }
        _ => None,
    }
}

fn release_download_url(tag: &str, asset: &str) -> String {
    format!("https://github.com/{GITHUB_REPO}/releases/download/{tag}/{asset}")
}

fn download_bytes(url: &str) -> Result<Vec<u8>> {
    let mut response = ureq::get(url).call().with_context(|| format!("GET {url}"))?;
    if !response.status().is_success() {
        bail!("GET {url} failed with HTTP {}", response.status());
    }
    response.body_mut().read_to_vec().with_context(|| format!("read body from {url}"))
}

fn fetch_text(url: &str) -> Result<String> {
    let bytes = download_bytes(url)?;
    String::from_utf8(bytes).with_context(|| format!("decode UTF-8 from {url}"))
}

/// Options for [`install_lsp`].
#[derive(Debug, Clone)]
pub struct InstallLspOptions {
    pub release_tag: String,
}

pub struct LspInstallResult {
    pub path: PathBuf,
    pub version: String,
    pub release_tag: String,
}

pub fn install_lsp(options: &InstallLspOptions) -> Result<LspInstallResult> {
    let tag = options.release_tag.trim();
    if tag.is_empty() {
        bail!("release tag must not be empty");
    }
    let asset = resolve_lsp_platform_asset()
        .with_context(|| format!("no published LSP build for {}-{}", std::env::consts::OS, std::env::consts::ARCH))?;

    let version_url = release_download_url(tag, "lsp-version.txt");
    let download_url = release_download_url(tag, asset.release_asset);
    let install_dir = home_beskid_bin_dir()?;
    let install_path = install_dir.join(asset.install_file_name);

    println!("Fetching version from {version_url}");
    let version = fetch_text(&version_url).context("read lsp-version.txt")?;
    if version.is_empty() {
        bail!("lsp-version.txt from {version_url} was empty");
    }

    println!("Downloading {download_url}");
    let bytes = download_bytes(&download_url)?;
    fs::create_dir_all(&install_dir).with_context(|| install_dir.display().to_string())?;
    let mut file = fs::File::create(&install_path).with_context(|| install_path.display().to_string())?;
    file.write_all(&bytes).with_context(|| install_path.display().to_string())?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&install_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&install_path, perms)?;
    }

    println!("Installed Beskid LSP {version} ({tag}) → {}", install_path.display());
    Ok(LspInstallResult { path: install_path, version, release_tag: tag.to_string() })
}

pub fn managed_lsp_exists() -> bool {
    managed_lsp_path().ok().is_some_and(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lsp_release_download_url_uses_github_release_layout() {
        let url = release_download_url("lsp-stable", "beskid_lsp-linux-amd64");
        assert_eq!(
            url,
            "https://github.com/Cyber-Nomad-Collective/beskid_compiler/releases/download/lsp-stable/beskid_lsp-linux-amd64"
        );
    }

    #[test]
    fn resolve_lsp_platform_asset_known_matrix() {
        let asset = resolve_lsp_platform_asset();
        let os = std::env::consts::OS;
        let arch = std::env::consts::ARCH;
        match (os, arch) {
            ("linux", "x86_64") => {
                let a = asset.expect("linux x64 asset");
                assert_eq!(a.release_asset, "beskid_lsp-linux-amd64");
            }
            ("macos", "aarch64") => {
                let a = asset.expect("darwin arm64 asset");
                assert_eq!(a.release_asset, "beskid_lsp-darwin-arm64");
            }
            ("windows", "x86_64") => {
                let a = asset.expect("windows x64 asset");
                assert_eq!(a.install_file_name, "beskid_lsp.exe");
            }
            _ => assert!(asset.is_none()),
        }
    }

    #[test]
    fn managed_lsp_path_is_under_beskid_bin() {
        let path = managed_lsp_path().expect("home set in test env");
        assert!(path.to_string_lossy().contains(".beskid"));
        assert!(path.to_string_lossy().contains("beskid_lsp"));
    }
}
