use std::collections::BTreeMap;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::{fs, io};

use clap::{Args, Subcommand};
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Attribute, Cell, Color, ContentArrangement, Table};
use indicatif::{ProgressBar, ProgressStyle};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::models::PackageVersionSummaryResponse;
use crate::{PckgClient, PckgClientConfig, PckgError};

const DEFAULT_PCKG_CONFIG_PATH: &str = ".beskid/pckg/repositories.json";

#[derive(Debug, Default, Serialize, Deserialize)]
struct PckgRepositoriesConfig {
    #[serde(default)]
    repositories: BTreeMap<String, RepositoryAuthConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RepositoryAuthConfig {
    api_key: String,
}

#[derive(Args, Debug, Clone)]
pub struct PckgArgs {
    /// pckg server base URL.
    #[arg(long, env = "BESKID_PCKG_URL", default_value = "http://127.0.0.1:8082")]
    pub base_url: String,

    /// Bearer token for authenticated endpoints.
    #[arg(long, env = "BESKID_PCKG_TOKEN", conflicts_with = "api_key")]
    pub bearer_token: Option<String>,

    /// Publisher API key for authenticated endpoints.
    #[arg(long, env = "BESKID_PCKG_API_KEY", conflicts_with = "bearer_token")]
    pub api_key: Option<String>,

    /// Request timeout in seconds.
    #[arg(long, default_value_t = 30)]
    pub timeout_secs: u64,

    /// Repository-local pckg config file path.
    #[arg(long, default_value = DEFAULT_PCKG_CONFIG_PATH)]
    pub config_file: PathBuf,

    /// Extra diagnostics (base URL, auth presence, timings).
    #[arg(short, long, default_value_t = false)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: PckgCommand,
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct PackVersionState {
    versions: BTreeMap<String, String>,
}

fn resolve_pack_version(source: &Path, args: &PackArgs) -> Result<String, PckgError> {
    let source_manifest_version = read_source_manifest_version(source)?;
    let stored_version = read_stored_pack_version(source, args)?;

    let baseline = max_version(source_manifest_version.as_ref(), stored_version.as_ref());
    let auto_version = bump_patch(baseline)?;

    match args.version.as_deref() {
        Some(explicit) => {
            let explicit_version = parse_version(explicit)?;
            if explicit_version <= auto_version {
                return Err(PckgError::Api {
                    status: reqwest::StatusCode::BAD_REQUEST,
                    message: format!(
                        "explicit version '{}' must be higher than auto-resolved '{}'",
                        explicit_version, auto_version
                    ),
                    body: None,
                });
            }

            Ok(explicit_version.to_string())
        }
        None => Ok(auto_version.to_string()),
    }
}

fn bump_patch(base: Option<Version>) -> Result<Version, PckgError> {
    let mut version = base.unwrap_or_else(|| Version::new(0, 1, 0));
    version.patch = version.patch.checked_add(1).ok_or_else(|| PckgError::Api {
        status: reqwest::StatusCode::BAD_REQUEST,
        message: "cannot bump patch version beyond supported range".to_string(),
        body: None,
    })?;
    version.pre = semver::Prerelease::EMPTY;
    version.build = semver::BuildMetadata::EMPTY;
    Ok(version)
}

fn max_version(a: Option<&Version>, b: Option<&Version>) -> Option<Version> {
    match (a, b) {
        (Some(left), Some(right)) => Some(if left >= right {
            left.clone()
        } else {
            right.clone()
        }),
        (Some(left), None) => Some(left.clone()),
        (None, Some(right)) => Some(right.clone()),
        (None, None) => None,
    }
}

fn parse_version(raw: &str) -> Result<Version, PckgError> {
    Version::parse(raw.trim()).map_err(|source| PckgError::Api {
        status: reqwest::StatusCode::BAD_REQUEST,
        message: format!("invalid semantic version '{}': {source}", raw.trim()),
        body: None,
    })
}

fn read_source_manifest_version(source: &Path) -> Result<Option<Version>, PckgError> {
    let manifest_path = source.join("package.json");
    if !manifest_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&manifest_path)?;
    let value: serde_json::Value =
        serde_json::from_str(&content).map_err(|source| PckgError::Api {
            status: reqwest::StatusCode::BAD_REQUEST,
            message: format!("failed to parse package.json: {source}"),
            body: None,
        })?;

    let Some(version_str) = value.get("version").and_then(serde_json::Value::as_str) else {
        return Ok(None);
    };

    Ok(Some(parse_version(version_str)?))
}

fn version_state_path(source: &Path, args: &PackArgs) -> PathBuf {
    if args.version_state_file.is_absolute() {
        args.version_state_file.clone()
    } else {
        source.join(&args.version_state_file)
    }
}

fn read_stored_pack_version(source: &Path, args: &PackArgs) -> Result<Option<Version>, PckgError> {
    let path = version_state_path(source, args);
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path)?;
    let state: PackVersionState =
        serde_json::from_str(&content).map_err(|source| PckgError::Api {
            status: reqwest::StatusCode::BAD_REQUEST,
            message: format!("failed to parse version state file: {source}"),
            body: None,
        })?;

    let Some(version) = state.versions.get(&args.package) else {
        return Ok(None);
    };

    Ok(Some(parse_version(version)?))
}

fn persist_pack_version_state(
    source: &Path,
    args: &PackArgs,
    version: &str,
) -> Result<(), PckgError> {
    let path = version_state_path(source, args);
    let mut state = if path.exists() {
        let content = fs::read_to_string(&path)?;
        serde_json::from_str::<PackVersionState>(&content).map_err(|source| PckgError::Api {
            status: reqwest::StatusCode::BAD_REQUEST,
            message: format!("failed to parse version state file: {source}"),
            body: None,
        })?
    } else {
        PackVersionState::default()
    };

    state
        .versions
        .insert(args.package.clone(), version.to_string());

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let output = serde_json::to_string_pretty(&state).map_err(|source| PckgError::Api {
        status: reqwest::StatusCode::INTERNAL_SERVER_ERROR,
        message: format!("failed to serialize version state: {source}"),
        body: None,
    })?;

    fs::write(path, output + "\n")?;
    Ok(())
}

fn print_package_versions_table(versions: &[PackageVersionSummaryResponse]) {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("Package").add_attribute(Attribute::Bold),
            Cell::new("Version").add_attribute(Attribute::Bold),
            Cell::new("Yanked").add_attribute(Attribute::Bold),
            Cell::new("Checksum").add_attribute(Attribute::Bold),
            Cell::new("Size").add_attribute(Attribute::Bold),
            Cell::new("Published").add_attribute(Attribute::Bold),
        ]);

    for version in versions {
        table.add_row(vec![
            Cell::new(&version.package_name),
            Cell::new(&version.version).fg(Color::Cyan),
            Cell::new(if version.is_yanked { "yes" } else { "no" }),
            Cell::new(&version.checksum_sha256),
            Cell::new(version.size_bytes.to_string()),
            Cell::new(&version.published_at_utc),
        ]);
    }

    println!("{table}");
}

#[derive(Subcommand, Debug, Clone)]
pub enum PckgCommand {
    /// Build a publishable .bpk artifact from a package directory.
    Pack(PackArgs),

    /// Upload and publish a package artifact version.
    Upload(PublishArgs),

    /// Save repository-local API key config used by upload commands.
    Configure(ConfigureArgs),

    /// List visible packages.
    List,

    /// Search packages by free-text query.
    Search(SearchArgs),

    /// Show package details by id or name.
    Details(DetailsArgs),

    /// List package versions by package name.
    Versions(VersionsArgs),

    /// Download an artifact version to file.
    Download(DownloadArgs),

    /// Yank a package version.
    Yank(VersionActionArgs),

    /// Restore a previously yanked package version.
    Unyank(VersionActionArgs),

    /// Print current authenticated user profile.
    Whoami,
}

#[derive(Args, Debug, Clone)]
pub struct ConfigureArgs {
    /// Repository URL to associate with API key.
    ///
    /// Defaults to --base-url when omitted.
    #[arg(long)]
    pub repository_url: Option<String>,

    /// API key to persist in repository config.
    #[arg(long)]
    pub api_key: String,
}

#[derive(Args, Debug, Clone)]
pub struct PublishArgs {
    pub package: String,
    #[arg(long)]
    pub version: String,
    #[arg(long)]
    pub artifact: PathBuf,
    #[arg(long)]
    pub checksum_sha256: Option<String>,
    #[arg(long)]
    pub manifest_json: Option<String>,
}

#[derive(Args, Debug, Clone)]
pub struct SearchArgs {
    pub query: String,
}

#[derive(Args, Debug, Clone)]
pub struct DetailsArgs {
    pub id_or_name: String,
}

#[derive(Args, Debug, Clone)]
pub struct VersionsArgs {
    pub package: String,
}

#[derive(Args, Debug, Clone)]
pub struct DownloadArgs {
    pub package: String,
    #[arg(long)]
    pub version: String,
    #[arg(long)]
    pub output: PathBuf,
}

#[derive(Args, Debug, Clone)]
pub struct VersionActionArgs {
    pub package: String,
    #[arg(long)]
    pub version: String,
}

#[derive(Args, Debug, Clone)]
pub struct PackArgs {
    #[arg(long)]
    pub package: String,
    #[arg(long)]
    pub version: Option<String>,
    #[arg(long, default_value = ".")]
    pub source: PathBuf,
    #[arg(long)]
    pub output: PathBuf,
    #[arg(long, default_value = ".beskid/pckg-version-state.json")]
    pub version_state_file: PathBuf,
}

pub fn execute(args: PckgArgs) -> Result<(), PckgError> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|err| PckgError::RuntimeInit(err.to_string()))?;

    runtime.block_on(execute_async(args))
}

async fn execute_async(args: PckgArgs) -> Result<(), PckgError> {
    let args_for_client = args.clone();
    if args.verbose {
        let auth = if args.api_key.is_some() || args.bearer_token.is_some() {
            "cli-args"
        } else {
            "repositories.json-or-env"
        };
        eprintln!(
            "[pckg] verbose: base_url={} auth_hint={auth}",
            args.base_url.trim()
        );
    }

    match args.command {
        PckgCommand::Pack(pack_args) => execute_pack(pack_args),
        PckgCommand::Upload(upload_args) => {
            let client = build_client(&args_for_client)?;
            execute_publish(&client, upload_args, args_for_client.verbose).await
        }
        PckgCommand::Configure(configure_args) => {
            execute_configure(&args.config_file, &args.base_url, configure_args)
        }
        PckgCommand::List => {
            let client = build_client(&args_for_client)?;
            execute_list(&client).await
        }
        PckgCommand::Search(search_args) => {
            let client = build_client(&args_for_client)?;
            execute_search(&client, search_args).await
        }
        PckgCommand::Details(details_args) => {
            let client = build_client(&args_for_client)?;
            execute_details(&client, details_args).await
        }
        PckgCommand::Versions(versions_args) => {
            let client = build_client(&args_for_client)?;
            execute_versions(&client, versions_args).await
        }
        PckgCommand::Download(download_args) => {
            let client = build_client(&args_for_client)?;
            execute_download(&client, download_args).await
        }
        PckgCommand::Yank(action_args) => {
            let client = build_client(&args_for_client)?;
            execute_yank(&client, action_args).await
        }
        PckgCommand::Unyank(action_args) => {
            let client = build_client(&args_for_client)?;
            execute_unyank(&client, action_args).await
        }
        PckgCommand::Whoami => {
            let client = build_client(&args_for_client)?;
            execute_whoami(&client).await
        }
    }
}

fn execute_configure(
    config_path: &Path,
    base_url: &str,
    args: ConfigureArgs,
) -> Result<(), PckgError> {
    let repository_url = args.repository_url.as_deref().unwrap_or(base_url).trim();

    if args.api_key.trim().is_empty() {
        return Err(PckgError::Api {
            status: reqwest::StatusCode::BAD_REQUEST,
            message: "api key cannot be empty".to_string(),
            body: None,
        });
    }

    save_repository_api_key(config_path, repository_url, args.api_key.trim())?;

    println!(
        "Saved API key for repository {} in {}. This config is loaded automatically by `pckg upload`.",
        repository_url,
        config_path.display(),
    );
    Ok(())
}

fn execute_pack(args: PackArgs) -> Result<(), PckgError> {
    let source = args.source.clone();
    let output = args.output.clone();
    let resolved_version = resolve_pack_version(&source, &args)?;

    let entries = collect_pack_entries(&source)?;
    if entries.is_empty() {
        return Err(PckgError::Api {
            status: reqwest::StatusCode::BAD_REQUEST,
            message: "no files found to package".to_string(),
            body: None,
        });
    }

    let package_json = serde_json::to_string_pretty(&serde_json::json!({
        "schema": "beskid.package.v1",
        "id": args.package,
        "version": resolved_version,
    }))
    .map_err(|source| PckgError::Api {
        status: reqwest::StatusCode::INTERNAL_SERVER_ERROR,
        message: format!("failed to serialize package.json: {source}"),
        body: None,
    })?;

    let mut checksums = BTreeMap::new();
    for (name, content) in &entries {
        checksums.insert(name.clone(), sha256_hex(content));
    }
    checksums.insert(
        "package.json".to_string(),
        sha256_hex(package_json.as_bytes()),
    );

    let checksums_sha = checksums
        .iter()
        .map(|(path, digest)| format!("{digest}  {path}"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";

    let file = fs::File::create(&output)?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    for (name, content) in entries {
        writer
            .start_file(name, options)
            .map_err(zip_to_pckg_error)?;
        writer.write_all(&content)?;
    }

    writer
        .start_file("package.json", options)
        .map_err(zip_to_pckg_error)?;
    writer.write_all(package_json.as_bytes())?;

    writer
        .start_file("checksums.sha256", options)
        .map_err(zip_to_pckg_error)?;
    writer.write_all(checksums_sha.as_bytes())?;

    writer.finish().map_err(zip_to_pckg_error)?;
    persist_pack_version_state(&source, &args, &resolved_version)?;
    println!("Resolved package version: {resolved_version}");
    println!("Packed artifact at {}", output.display());

    Ok(())
}

fn build_client(args: &PckgArgs) -> Result<PckgClient, PckgError> {
    let mut config =
        PckgClientConfig::new(&args.base_url)?.with_timeout(Duration::from_secs(args.timeout_secs));

    if let Some(token) = args.bearer_token.as_ref() {
        config = config.with_bearer_token(token.clone());
    } else if let Some(api_key) = args.api_key.clone().or_else(|| {
        read_saved_api_key(&args.config_file, &args.base_url)
            .ok()
            .flatten()
    }) {
        config = config.with_publisher_api_key(api_key.clone());
    }

    PckgClient::new(config)
}

fn save_repository_api_key(
    config_path: &Path,
    repository_url: &str,
    api_key: &str,
) -> Result<(), PckgError> {
    let canonical_url = canonical_repository_url(repository_url)?;
    let mut config = load_repositories_config(config_path)?;
    config.repositories.insert(
        canonical_url,
        RepositoryAuthConfig {
            api_key: api_key.to_string(),
        },
    );

    if let Some(parent) = config_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }

    let mut output = serde_json::to_string_pretty(&config).map_err(|source| PckgError::Api {
        status: reqwest::StatusCode::INTERNAL_SERVER_ERROR,
        message: format!("failed to serialize pckg repositories config: {source}"),
        body: None,
    })?;
    output.push('\n');
    fs::write(config_path, output)?;

    #[cfg(unix)]
    {
        let permissions = fs::Permissions::from_mode(0o600);
        fs::set_permissions(config_path, permissions)?;
    }

    Ok(())
}

fn read_saved_api_key(config_path: &Path, base_url: &str) -> Result<Option<String>, PckgError> {
    let canonical_url = canonical_repository_url(base_url)?;
    let config = load_repositories_config(config_path)?;
    Ok(config
        .repositories
        .get(&canonical_url)
        .map(|entry| entry.api_key.clone())
        .filter(|value| !value.trim().is_empty()))
}

fn load_repositories_config(config_path: &Path) -> Result<PckgRepositoriesConfig, PckgError> {
    if !config_path.exists() {
        return Ok(PckgRepositoriesConfig::default());
    }

    let content = fs::read_to_string(config_path)?;
    match serde_json::from_str::<PckgRepositoriesConfig>(&content) {
        Ok(config) => Ok(config),
        Err(_) => {
            let legacy_key = content
                .lines()
                .map(str::trim)
                .find_map(|line| line.strip_prefix("BESKID_PCKG_API_KEY="))
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);

            if let Some(legacy_key) = legacy_key {
                let mut repositories = BTreeMap::new();
                let default_repository = canonical_repository_url("http://127.0.0.1:8082")?;
                repositories.insert(
                    default_repository,
                    RepositoryAuthConfig {
                        api_key: legacy_key,
                    },
                );
                Ok(PckgRepositoriesConfig { repositories })
            } else {
                Ok(PckgRepositoriesConfig::default())
            }
        }
    }
}

fn canonical_repository_url(raw_url: &str) -> Result<String, PckgError> {
    let mut url = Url::parse(raw_url).map_err(|source| PckgError::Api {
        status: reqwest::StatusCode::BAD_REQUEST,
        message: format!("invalid repository url '{raw_url}': {source}"),
        body: None,
    })?;
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path().trim_end_matches('/'));
        url.set_path(&path);
    }

    Ok(url.to_string())
}

async fn execute_publish(
    client: &PckgClient,
    args: PublishArgs,
    verbose: bool,
) -> Result<(), PckgError> {
    let artifact_name = args
        .artifact
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("artifact.bpk")
        .to_string();

    let artifact_path = &args.artifact;
    let len = tokio::fs::metadata(artifact_path)
        .await
        .map_err(PckgError::Io)?
        .len();

    let upload_pb = if io::stdout().is_terminal() && len > 0 {
        let pb = ProgressBar::new(len);
        pb.set_style(
            ProgressStyle::with_template(
                "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})",
            )
            .expect("template")
            .progress_chars("#>-"),
        );
        pb.set_message("uploading artifact");
        Some(pb)
    } else {
        None
    };

    let spinner = if upload_pb.is_none() {
        Some(spinner("Publishing package version..."))
    } else {
        None
    };

    let started = Instant::now();
    let response = client
        .publish_package_version(
            &args.package,
            &args.version,
            artifact_path,
            &artifact_name,
            args.manifest_json.as_deref(),
            args.checksum_sha256.as_deref(),
            upload_pb.as_ref(),
        )
        .await;

    if let Some(s) = spinner.as_ref() {
        match &response {
            Ok(_) => s.finish_with_message("Package publish request completed."),
            Err(_) => s.abandon_with_message("Package publish failed."),
        }
    }

    match response {
        Ok(response) => {
            if verbose {
                eprintln!("[pckg] verbose: upload elapsed {:?}", started.elapsed());
            }
            let base = client.config().base_url.as_str().trim_end_matches('/');
            println!("{}", response.message);
            println!("--- publish summary ---");
            println!("registry: {base}");
            println!(
                "request:  POST /api/packages/{}/publish",
                args.package.trim()
            );
            println!("package:  {}", args.package);
            println!("version:  {}", args.version);
            if let Some(version) = &response.version {
                println!("checksum: {}", version.checksum_sha256);
                println!("size:     {} bytes", version.size_bytes);
                println!("published_at_utc: {}", version.published_at_utc);
                print_package_versions_table(std::slice::from_ref(version));
            } else {
                println!("(no version details in response)");
            }
            println!("------------------------");
            Ok(())
        }
        Err(err) => {
            print_pckg_error_human(&err);
            Err(err)
        }
    }
}

fn print_pckg_error_human(err: &PckgError) {
    eprintln!("pckg error: {err}");
    match err {
        PckgError::Api { body: Some(b), .. } | PckgError::LogicalFailure { body: Some(b), .. } => {
            let snippet: String = b.chars().take(2000).collect();
            if !snippet.is_empty() {
                eprintln!("response body (truncated): {snippet}");
            }
        }
        _ => {}
    }
}

async fn execute_list(client: &PckgClient) -> Result<(), PckgError> {
    let items = client.list_packages().await?;
    if items.is_empty() {
        println!("No packages found.");
        return Ok(());
    }

    for item in items {
        println!(
            "{} [{}] downloads={} rating={:.2}",
            item.name, item.category, item.total_downloads, item.average_rating
        );
    }
    Ok(())
}

async fn execute_search(client: &PckgClient, args: SearchArgs) -> Result<(), PckgError> {
    let items = client.search_packages(&args.query).await?;
    if items.is_empty() {
        println!("No packages matched '{}'.", args.query);
        return Ok(());
    }

    for item in items {
        println!(
            "{} [{}/{}] score={:.2} reviews={}",
            item.package.name,
            item.health.state,
            item.health.sub_state,
            item.health.score,
            item.review_count
        );
    }
    Ok(())
}

async fn execute_details(client: &PckgClient, args: DetailsArgs) -> Result<(), PckgError> {
    let details = client.get_package_details(&args.id_or_name).await?;
    println!(
        "{} ({}) downloads={} dependents={}",
        details.package.name,
        details.package.category,
        details.package.total_downloads,
        details.dependents_count
    );
    println!(
        "health={}/{} score={:.2}",
        details.health.state, details.health.sub_state, details.health.score
    );
    if !details.dependencies.is_empty() {
        println!("dependencies:");
        for dep in details.dependencies {
            println!(
                "- {} {} source={} registry={}",
                dep.name,
                dep.version.unwrap_or_else(|| "*".to_string()),
                dep.source,
                dep.registry.unwrap_or_else(|| "-".to_string())
            );
        }
    }
    Ok(())
}

async fn execute_versions(client: &PckgClient, args: VersionsArgs) -> Result<(), PckgError> {
    let versions = client.list_package_versions(&args.package).await?;
    if versions.is_empty() {
        println!("No versions found for {}.", args.package);
        return Ok(());
    }
    print_package_versions_table(&versions);
    Ok(())
}

async fn execute_download(client: &PckgClient, args: DownloadArgs) -> Result<(), PckgError> {
    let bytes = client
        .download_package_version(&args.package, &args.version)
        .await?;
    if let Some(parent) = args.output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&args.output, bytes)?;
    println!(
        "Downloaded {} {} to {}",
        args.package,
        args.version,
        args.output.display()
    );
    Ok(())
}

async fn execute_yank(client: &PckgClient, args: VersionActionArgs) -> Result<(), PckgError> {
    let response = client
        .yank_package_version(&args.package, &args.version)
        .await?;
    println!("{}", response.message);
    Ok(())
}

async fn execute_unyank(client: &PckgClient, args: VersionActionArgs) -> Result<(), PckgError> {
    let response = client
        .unyank_package_version(&args.package, &args.version)
        .await?;
    println!("{}", response.message);
    Ok(())
}

async fn execute_whoami(client: &PckgClient) -> Result<(), PckgError> {
    let me = client.current_user().await?;
    println!(
        "authenticated={} user_id={} email={} publisher={}",
        me.is_authenticated,
        me.user_id.unwrap_or_else(|| "-".to_string()),
        me.email.unwrap_or_else(|| "-".to_string()),
        me.is_publisher
    );
    Ok(())
}

fn collect_pack_entries(source_root: &Path) -> Result<Vec<(String, Vec<u8>)>, PckgError> {
    let mut entries = Vec::new();

    for entry in WalkDir::new(source_root).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let rel_path = path.strip_prefix(source_root).map_err(io::Error::other)?;
        let rel = normalize_rel_path(rel_path);

        if rel == "checksums.sha256" || rel == "package.json" {
            continue;
        }

        let bytes = fs::read(path)?;
        entries.push((rel, bytes));
    }

    entries.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(entries)
}

fn normalize_rel_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let hash = hasher.finalize();
    format!("{hash:x}")
}

fn zip_to_pckg_error(source: zip::result::ZipError) -> PckgError {
    PckgError::Api {
        status: reqwest::StatusCode::INTERNAL_SERVER_ERROR,
        message: format!("zip packaging error: {source}"),
        body: None,
    }
}

fn spinner(message: &str) -> ProgressBar {
    let spinner = ProgressBar::new_spinner();
    if let Ok(style) = ProgressStyle::with_template("{spinner:.green} {msg}") {
        spinner.set_style(style);
    }
    spinner.enable_steady_tick(Duration::from_millis(90));
    spinner.set_message(message.to_string());
    spinner
}
