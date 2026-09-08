//! Public package commands (`get`, `search`, `install`, `remove`) and full legacy `pckg` proxy.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use clap::{Args, Subcommand};
use walkdir::WalkDir;

use crate::project_args::{LockfilePolicyArgs, ProjectResolveArgs};

use beskid_analysis::projects::discover_project_manifest_in_dir;
use beskid_pckg::{
    cli::{PckgArgs, PckgCommand},
    models::{PackageDetailsResponse, PackageSearchResponse, PackageVersionSummaryResponse},
    PckgClient, PckgClientConfig, PckgError,
};
use beskid_tools::registry::{latest_non_yanked, parse_package_selector, pick_version};

pub use beskid_pckg::cli::PckgArgs as PackagePckgArgs;

#[derive(Args, Debug, Clone)]
pub struct RegistryConnectArgs {
    /// Registry base URL.
    #[arg(long, env = "BESKID_PCKG_URL", default_value = "https://pckg.beskid-lang.org")]
    pub base_url: String,

    /// Bearer token for authenticated endpoints.
    #[arg(long, env = "BESKID_PCKG_TOKEN", conflicts_with = "api_key")]
    pub bearer_token: Option<String>,

    /// Publisher API key for authenticated endpoints.
    #[arg(long, env = "BESKID_PCKG_API_KEY", conflicts_with = "bearer_token")]
    pub api_key: Option<String>,

    /// Client request timeout in seconds.
    #[arg(long, default_value_t = 30)]
    pub timeout_secs: u64,
}

#[derive(Args, Debug, Clone)]
pub struct PackageGetArgs {
    /// Package selector: `<package-id>` or `<package-id>@<version>`.
    pub selector: String,

    #[command(flatten)]
    pub registry: RegistryConnectArgs,
}

#[derive(Args, Debug, Clone)]
pub struct PackageSearchArgs {
    /// Search query.
    pub query: String,

    #[command(flatten)]
    pub registry: RegistryConnectArgs,
}

#[derive(Args, Debug, Clone)]
pub struct PackageInstallArgs {
    /// Package selector: `<package-id>` or `<package-id>@<version>`.
    pub selector: String,

    /// Destination path (defaults to local package cache).
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Overwrite existing destination path.
    #[arg(long)]
    pub force: bool,

    #[command(flatten)]
    pub registry: RegistryConnectArgs,
}

#[derive(Args, Debug, Clone)]
pub struct PackageRemoveArgs {
    /// Package selector: `<package-id>` or `<package-id>@<version>`.
    pub selector: String,

    /// Remove all cached versions for the package.
    #[arg(long)]
    pub all_versions: bool,

    #[command(flatten)]
    pub registry: RegistryConnectArgs,
}

#[derive(Subcommand, Debug)]
pub enum PackageDevCommand {
    /// Full legacy `beskid pckg` command tree.
    Pckg(PackagePckgArgs),
}

pub fn get_command(args: PackageGetArgs) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to create tokio runtime")?;

    let (package_id, requested_version) = parse_selector(&args.selector)?;
    let client = build_client(&args.registry)?;

    runtime.block_on(execute_get(&client, package_id, requested_version))
}

pub fn search_command(args: PackageSearchArgs) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to create tokio runtime")?;

    let client = build_client(&args.registry)?;

    runtime.block_on(execute_search(&client, args.query))
}

pub fn install_command(mut args: PackageInstallArgs) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to create tokio runtime")?;

    let client = build_client(&args.registry)?;

    runtime.block_on(async move { execute_install(&client, &mut args).await })
}

pub fn remove_command(args: PackageRemoveArgs) -> Result<()> {
    let (package_id, requested_version) = parse_selector(&args.selector)?;
    let cached = local_cached_versions(&package_id)?;

    if cached.is_empty() {
        anyhow::bail!("package `{package_id}` has no local cache entries");
    }

    if let Some(version) = requested_version {
        let path = package_version_path(&package_id, &version);
        if !path.exists() {
            anyhow::bail!("{package_id}@{version} not present in local cache");
        }
        std::fs::remove_file(&path).context(format!("failed to remove {}", path.display()))?;
        println!("removed {package_id}@{version} from {}", path.display());
        return Ok(());
    }

    if args.all_versions {
        let root = package_root(&package_id);
        std::fs::remove_dir_all(&root).context(format!("failed to remove {}", root.display()))?;
        println!("removed all cache entries for `{}`", package_id);
        return Ok(());
    }

    if cached.len() == 1 {
        let version = &cached[0];
        let path = package_version_path(&package_id, version);
        std::fs::remove_file(&path).context(format!("failed to remove {}", path.display()))?;
        println!("removed {package_id}@{version} from {}", path.display());
        return Ok(());
    }

    anyhow::bail!(
        "package `{package_id}` has multiple cached versions ({}). pass `--all-versions` or `@<version>`",
        cached.join(", ")
    );
}

pub fn execute_pckg_proxy(args: PackagePckgArgs) -> Result<()> {
    maybe_generate_docs_for_pack(&args)?;
    pckg::cli::execute(args).map_err(Into::<anyhow::Error>::into)
}

fn build_client(args: &RegistryConnectArgs) -> anyhow::Result<PckgClient> {
    let mut config = PckgClientConfig::new(&args.base_url).map_err(|err| anyhow!("invalid BESKID_PCKG_URL: {err}"))?;
    config = config.with_timeout(Duration::from_secs(args.timeout_secs));
    let config = if let Some(token) = &args.bearer_token {
        config.with_bearer_token(token)
    } else if let Some(api_key) = &args.api_key {
        config.with_publisher_api_key(api_key)
    } else {
        config
    };
    Ok(PckgClient::new(config).map_err(convert_pckg_error)?)
}

fn parse_selector(raw: &str) -> Result<(String, Option<String>)> {
    let (package_id, version) = parse_package_selector(raw)?;
    if package_id.trim().is_empty() {
        anyhow::bail!("package selector must be non-empty");
    }
    Ok((package_id, version))
}

async fn execute_get(client: &PckgClient, package_id: String, version: Option<String>) -> Result<()> {
    let details = client.get_package_details(&package_id).await.map_err(convert_pckg_error)?;
    print_package_details(&details);
    match version {
        Some(version) => {
            let selected =
                details.versions.iter().find(|candidate| candidate.version == version).ok_or_else(|| {
                    anyhow!("version `{version}` not found for package `{package_id}`")
                })?;
            print_version_details(selected);
        }
        None => {
            println!("\nAvailable versions:");
            for item in details.versions.iter() {
                print_version_summary(item);
            }
        }
    }
    Ok(())
}

async fn execute_search(client: &PckgClient, query: String) -> Result<()> {
    let matches = client.search_packages(&query).await.map_err(convert_pckg_error)?;
    if matches.is_empty() {
        println!("No packages matched `{query}`.");
        return Ok(());
    }

    println!("Registry matches:");
    for item in matches {
        print_search_result(&item);
    }
    Ok(())
}

async fn execute_install(client: &PckgClient, args: &mut PackageInstallArgs) -> Result<()> {
    let (package_id, requested_version) = parse_selector(&args.selector)?;
    let versions = client.list_package_versions(&package_id).await.map_err(convert_pckg_error)?;
    let version = pick_install_version(requested_version.as_deref(), &versions)?;

    let output = args.output.clone().or_else(|| Some(package_version_path(&package_id, &version)));
    let output = output.expect("install output path");
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    }

    if output.exists() && !args.force {
        anyhow::bail!("artifact already exists at {}. pass --force to overwrite", output.display());
    }

    let bytes = client.download_package_version(&package_id, &version).await.map_err(convert_pckg_error)?;
    std::fs::write(&output, bytes).with_context(|| format!("failed to write {}", output.display()))?;
    println!("installed {package_id}@{version} to {}", output.display());
    Ok(())
}

fn pick_install_version(requested: Option<&str>, versions: &[PackageVersionSummaryResponse]) -> Result<String> {
    if versions.is_empty() {
        anyhow::bail!("no published versions found");
    }
    if let Some(version) = pick_version(versions, requested).map_err(|err| anyhow!("{err}"))? {
        return Ok(version.version);
    }
    latest_non_yanked(versions)
        .map(|version| version.version)
        .ok_or_else(|| anyhow!("no non-yanked version published yet"))
}

fn local_cached_versions(package_id: &str) -> Result<Vec<String>> {
    let root = package_root(package_id);
    let mut versions = Vec::new();
    if !root.exists() {
        return Ok(versions);
    }
    for entry in std::fs::read_dir(root)? {
        let path = entry?.path();
        let Some(file) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(suffix) = file.strip_prefix(&format!("{}@", sanitize_package_id(package_id))) else {
            continue;
        };
        if suffix.ends_with(".bpk") {
            versions.push(suffix.trim_end_matches(".bpk").to_string());
        }
    }
    versions.sort();
    Ok(versions)
}

fn package_root(package_id: &str) -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(".beskid").join("packages").join(sanitize_package_id(package_id))
}

fn package_version_path(package_id: &str, version: &str) -> PathBuf {
    package_root(package_id).join(format!("{}@{}.bpk", sanitize_package_id(package_id), version))
}

fn sanitize_package_id(value: &str) -> String {
    value.replace(['/', '\\', ':', ' '], "_")
}

fn print_search_result(result: &PackageSearchResponse) {
    println!(
        "{} [{}] score={:.2} downloads={}",
        result.package.name,
        result.health.state,
        result.health.score,
        result.package.total_downloads
    );
}

fn print_package_details(details: &PackageDetailsResponse) {
    println!("{} [{}] downloads={}", details.package.name, details.package.category, details.package.total_downloads);
    println!("versions: {}", details.versions.len());
    if !details.dependencies.is_empty() {
        println!("dependencies:");
        for dependency in &details.dependencies {
            println!(
                "  - {} {} source={} registry={}",
                dependency.name,
                dependency.version.clone().unwrap_or_else(|| "*".to_string()),
                dependency.source,
                dependency.registry.clone().unwrap_or_else(|| "-".to_string())
            );
        }
    }
}

fn print_version_details(version: &PackageVersionSummaryResponse) {
    println!("version detail for {}@{}", version.package_id, version.version);
    print_version_summary(version);
}

fn print_version_summary(version: &PackageVersionSummaryResponse) {
    println!(
        "  {} {} {}B {}",
        version.version,
        if version.is_yanked { "[yanked]" } else { "" },
        version.size_bytes,
        if version.is_yanked { "yanked" } else { "ok" }
    );
}

fn maybe_generate_docs_for_pack(args: &PckgArgs) -> anyhow::Result<()> {
    let PckgCommand::Pack(pack_args) = &args.command else {
        return Ok(());
    };

    let source_root = absolutize_source_root(&pack_args.source)?;
    if matches!(beskid_pckg::detect_pack_profile(&source_root)?, beskid_pckg::PackProfile::Template(_)) {
        return Ok(());
    }

    let (input, project) = resolve_doc_entrypoint(&source_root)?;
    let out = source_root.join(".beskid").join("docs");

    let doc_args = crate::commands::doc::DocArgs {
        input,
        project: ProjectResolveArgs { project, target: None, workspace_member: None },
        lockfile: LockfilePolicyArgs { frozen: false, locked: false },
        out,
    };
    crate::commands::doc::execute(doc_args)?;
    Ok(())
}

fn absolutize_source_root(source: &Path) -> anyhow::Result<PathBuf> {
    if source.is_absolute() {
        Ok(source.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(source))
    }
}

fn resolve_doc_entrypoint(source_root: &Path) -> anyhow::Result<(Option<PathBuf>, Option<PathBuf>)> {
    if let Ok(Some(project_manifest)) = discover_project_manifest_in_dir(source_root) {
        return Ok((None, Some(project_manifest)));
    }

    for candidate in [source_root.join("main.bd"), source_root.join("src").join("main.bd"), source_root.join("index.bd")] {
        if candidate.exists() {
            return Ok((Some(candidate), None));
        }
    }

    let mut bd_files: Vec<PathBuf> = WalkDir::new(source_root)
        .into_iter()
        .filter_map(Result::ok)
        .map(|entry| entry.path().to_path_buf())
        .filter(|path| path.is_file())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("bd"))
        .collect();
    bd_files.sort();
    if bd_files.len() == 1 {
        return Ok((Some(bd_files.remove(0)), None));
    }

    anyhow::bail!(
        "cannot infer docs entrypoint for package source {} (expected a `.bproj` manifest, main.bd/src/main.bd, or a single .bd file)",
        source_root.display()
    )
}

fn convert_pckg_error(error: PckgError) -> anyhow::Error {
    anyhow!("{}", error)
}

mod pckg {
    pub use beskid_pckg::cli::execute;
}
