use std::time::Duration;
use std::{fs, path::PathBuf};

use clap::{Args, Subcommand};
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Attribute, Cell, Color, ContentArrangement, Table};
use indicatif::{ProgressBar, ProgressStyle};

use crate::models::{
    CreateApiKeyRequest, LoginUserRequest, PackageSummaryResponse, PackageVersionSummaryResponse,
};
use crate::{PckgClient, PckgClientConfig, PckgError};

#[derive(Args, Debug, Clone)]
pub struct PckgArgs {
    /// pckg server base URL.
    #[arg(long, env = "BESKID_PCKG_URL", default_value = "http://127.0.0.1:5195")]
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

    #[command(subcommand)]
    pub command: PckgCommand,
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
    /// Print authenticated identity information.
    Whoami,

    /// Authenticate with user credentials.
    Login(LoginArgs),

    /// List all packages visible to current principal.
    List,

    /// Search packages by name/description (case-insensitive).
    Search(SearchArgs),

    /// List versions for a package.
    Versions(VersionsArgs),

    /// Publish a package artifact version.
    Publish(PublishArgs),

    /// Download a package artifact version.
    Download(DownloadArgs),

    /// API key management.
    Keys(KeysArgs),
}

#[derive(Args, Debug, Clone)]
pub struct LoginArgs {
    #[arg(long)]
    pub email: String,
    #[arg(long)]
    pub password: String,
    #[arg(long, default_value_t = true)]
    pub remember_me: bool,
}

#[derive(Args, Debug, Clone)]
pub struct SearchArgs {
    pub query: String,
}

#[derive(Args, Debug, Clone)]
pub struct VersionsArgs {
    pub package: String,
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
pub struct DownloadArgs {
    pub package: String,
    #[arg(long)]
    pub version: String,
    #[arg(long)]
    pub output: PathBuf,
}

#[derive(Args, Debug, Clone)]
pub struct KeysArgs {
    #[command(subcommand)]
    pub command: KeysCommand,
}

#[derive(Subcommand, Debug, Clone)]
pub enum KeysCommand {
    /// List API keys available for current user.
    List,

    /// Create a new API key.
    Create(CreateKeyArgs),
}

#[derive(Args, Debug, Clone)]
pub struct CreateKeyArgs {
    #[arg(long)]
    pub name: String,
    #[arg(long = "scope")]
    pub scopes: Vec<String>,
}

pub fn execute(args: PckgArgs) -> Result<(), PckgError> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|err| PckgError::RuntimeInit(err.to_string()))?;

    runtime.block_on(execute_async(args))
}

async fn execute_async(args: PckgArgs) -> Result<(), PckgError> {
    let client = build_client(&args)?;

    match args.command {
        PckgCommand::Whoami => execute_whoami(&client).await,
        PckgCommand::Login(login_args) => execute_login(&client, login_args).await,
        PckgCommand::List => execute_list(&client).await,
        PckgCommand::Search(search_args) => execute_search(&client, search_args).await,
        PckgCommand::Versions(versions_args) => execute_versions(&client, versions_args).await,
        PckgCommand::Publish(publish_args) => execute_publish(&client, publish_args).await,
        PckgCommand::Download(download_args) => execute_download(&client, download_args).await,
        PckgCommand::Keys(keys_args) => execute_keys(&client, keys_args).await,
    }
}

fn build_client(args: &PckgArgs) -> Result<PckgClient, PckgError> {
    let mut config = PckgClientConfig::new(&args.base_url)?
        .with_timeout(Duration::from_secs(args.timeout_secs));

    if let Some(token) = args.bearer_token.as_ref() {
        config = config.with_bearer_token(token.clone());
    }

    if let Some(api_key) = args.api_key.as_ref() {
        config = config.with_publisher_api_key(api_key.clone());
    }

    PckgClient::new(config)
}

async fn execute_whoami(client: &PckgClient) -> Result<(), PckgError> {
    let spinner = spinner("Resolving current identity...");
    let current = client.current_user().await;
    match current {
        Ok(current) => {
            spinner.finish_with_message("Identity resolved.");
            println!("Authenticated: {}", current.is_authenticated);
            println!("User ID: {}", current.user_id.as_deref().unwrap_or("<none>"));
            println!("Email: {}", current.email.as_deref().unwrap_or("<none>"));
            println!("Publisher: {}", current.is_publisher);
            Ok(())
        }
        Err(err) => {
            spinner.abandon_with_message("Identity lookup failed.");
            Err(err)
        }
    }
}

async fn execute_login(client: &PckgClient, args: LoginArgs) -> Result<(), PckgError> {
    let spinner = spinner("Submitting credentials...");
    let response = client
        .login_user(&LoginUserRequest {
            email: args.email,
            password: args.password,
            remember_me: args.remember_me,
        })
        .await;

    match response {
        Ok(response) => {
            spinner.finish_with_message("Login request completed.");
            println!("{}", response.message);
            Ok(())
        }
        Err(err) => {
            spinner.abandon_with_message("Login request failed.");
            Err(err)
        }
    }
}

async fn execute_list(client: &PckgClient) -> Result<(), PckgError> {
    let spinner = spinner("Fetching packages...");
    let packages = client.list_packages().await;
    match packages {
        Ok(packages) => {
            spinner.finish_with_message("Packages fetched.");
            print_packages_table(&packages);
            Ok(())
        }
        Err(err) => {
            spinner.abandon_with_message("Package fetch failed.");
            Err(err)
        }
    }
}

async fn execute_search(client: &PckgClient, args: SearchArgs) -> Result<(), PckgError> {
    let spinner = spinner("Searching packages...");
    let packages = client.list_packages().await;
    match packages {
        Ok(packages) => {
            let query = args.query.to_ascii_lowercase();
            let filtered: Vec<_> = packages
                .into_iter()
                .filter(|package| {
                    package.name.to_ascii_lowercase().contains(&query)
                        || package.description.to_ascii_lowercase().contains(&query)
                })
                .collect();

            spinner.finish_with_message(format!("Found {} matching packages.", filtered.len()));
            print_packages_table(&filtered);
            Ok(())
        }
        Err(err) => {
            spinner.abandon_with_message("Package search failed.");
            Err(err)
        }
    }
}

async fn execute_versions(client: &PckgClient, args: VersionsArgs) -> Result<(), PckgError> {
    let spinner = spinner("Fetching package versions...");
    let versions = client.list_package_versions(&args.package).await;
    match versions {
        Ok(versions) => {
            spinner.finish_with_message("Versions fetched.");
            print_package_versions_table(&versions);
            Ok(())
        }
        Err(err) => {
            spinner.abandon_with_message("Version listing failed.");
            Err(err)
        }
    }
}

async fn execute_publish(client: &PckgClient, args: PublishArgs) -> Result<(), PckgError> {
    let artifact_name = args
        .artifact
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("artifact.bpk")
        .to_string();

    let artifact_bytes = fs::read(&args.artifact).map_err(|source| PckgError::Api {
        status: reqwest::StatusCode::INTERNAL_SERVER_ERROR,
        message: format!("failed to read artifact: {source}"),
        body: None,
    })?;

    let spinner = spinner("Publishing package version...");
    let response = client
        .publish_package_version(
            &args.package,
            &args.version,
            &artifact_name,
            artifact_bytes,
            args.manifest_json.as_deref(),
            args.checksum_sha256.as_deref(),
        )
        .await;

    match response {
        Ok(response) => {
            spinner.finish_with_message("Package publish request completed.");
            println!("{}", response.message);
            if let Some(version) = response.version {
                print_package_versions_table(&[version]);
            }
            Ok(())
        }
        Err(err) => {
            spinner.abandon_with_message("Package publish failed.");
            Err(err)
        }
    }
}

async fn execute_download(client: &PckgClient, args: DownloadArgs) -> Result<(), PckgError> {
    let spinner = spinner("Downloading package artifact...");
    let artifact = client
        .download_package_version(&args.package, &args.version)
        .await;

    match artifact {
        Ok(artifact) => {
            fs::write(&args.output, artifact).map_err(|source| PckgError::Api {
                status: reqwest::StatusCode::INTERNAL_SERVER_ERROR,
                message: format!("failed to write downloaded artifact: {source}"),
                body: None,
            })?;
            spinner.finish_with_message("Artifact downloaded.");
            println!("Saved artifact to {}", args.output.display());
            Ok(())
        }
        Err(err) => {
            spinner.abandon_with_message("Artifact download failed.");
            Err(err)
        }
    }
}

async fn execute_keys(client: &PckgClient, args: KeysArgs) -> Result<(), PckgError> {
    match args.command {
        KeysCommand::List => execute_keys_list(client).await,
        KeysCommand::Create(create_args) => execute_keys_create(client, create_args).await,
    }
}

async fn execute_keys_list(client: &PckgClient) -> Result<(), PckgError> {
    let spinner = spinner("Fetching API keys...");
    let keys = client.list_api_keys().await;
    match keys {
        Ok(keys) => {
            spinner.finish_with_message("API keys fetched.");

            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .apply_modifier(UTF8_ROUND_CORNERS)
                .set_content_arrangement(ContentArrangement::Dynamic)
                .set_header(vec![
                    Cell::new("Name").add_attribute(Attribute::Bold),
                    Cell::new("Prefix").add_attribute(Attribute::Bold),
                    Cell::new("Scopes").add_attribute(Attribute::Bold),
                    Cell::new("Revoked").add_attribute(Attribute::Bold),
                ]);

            for key in keys {
                table.add_row(vec![
                    Cell::new(key.name),
                    Cell::new(key.prefix).fg(Color::Cyan),
                    Cell::new(key.scopes.join(", ")),
                    Cell::new(if key.revoked_at_utc.is_some() {
                        "yes"
                    } else {
                        "no"
                    }),
                ]);
            }

            println!("{table}");
            Ok(())
        }
        Err(err) => {
            spinner.abandon_with_message("API key fetch failed.");
            Err(err)
        }
    }
}

async fn execute_keys_create(client: &PckgClient, args: CreateKeyArgs) -> Result<(), PckgError> {
    let spinner = spinner("Creating API key...");
    let response = client
        .create_api_key(&CreateApiKeyRequest {
            name: args.name,
            scopes: (!args.scopes.is_empty()).then_some(args.scopes),
        })
        .await;

    match response {
        Ok(response) => {
            spinner.finish_with_message("API key created.");
            println!("{}", response.message);
            if let Some(plain) = response.plain_text_key {
                println!("New API key (store it now, it won't be shown again): {plain}");
            }
            Ok(())
        }
        Err(err) => {
            spinner.abandon_with_message("API key creation failed.");
            Err(err)
        }
    }
}

fn print_packages_table(packages: &[PackageSummaryResponse]) {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("Name").add_attribute(Attribute::Bold),
            Cell::new("Description").add_attribute(Attribute::Bold),
            Cell::new("Public").add_attribute(Attribute::Bold),
            Cell::new("Reviews").add_attribute(Attribute::Bold),
            Cell::new("Rating").add_attribute(Attribute::Bold),
        ]);

    for package in packages {
        table.add_row(vec![
            Cell::new(&package.name).fg(Color::Green),
            Cell::new(&package.description),
            Cell::new(if package.is_public { "yes" } else { "no" }),
            Cell::new(package.pending_reviews_count.to_string()),
            Cell::new(format!("{:.2}", package.average_rating)),
        ]);
    }

    println!("{table}");
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
