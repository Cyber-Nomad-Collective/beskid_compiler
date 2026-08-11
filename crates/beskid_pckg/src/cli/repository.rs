use super::{BTreeMap, ConfigureArgs, Duration, Path, PckgArgs, PckgClient, PckgClientConfig, PckgError, Url, fs};
use serde::{Deserialize, Serialize};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[derive(Debug, Default, Serialize, Deserialize)]
struct PckgRepositoriesConfig {
    #[serde(default)]
    repositories: BTreeMap<String, RepositoryAuthConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RepositoryAuthConfig {
    api_key: String,
}
pub(super) fn execute_configure(config_path: &Path, base_url: &str, args: ConfigureArgs) -> Result<(), PckgError> {
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
pub(super) fn build_client(args: &PckgArgs) -> Result<PckgClient, PckgError> {
    let mut config = PckgClientConfig::new(&args.base_url)?.with_timeout(Duration::from_secs(args.timeout_secs));

    if let Some(token) = args.bearer_token.as_ref() {
        config = config.with_bearer_token(token.clone());
    } else if let Some(api_key) =
        args.api_key.clone().or_else(|| read_saved_api_key(&args.config_file, &args.base_url).ok().flatten())
    {
        config = config.with_publisher_api_key(api_key.clone());
    }

    PckgClient::new(config)
}

fn save_repository_api_key(config_path: &Path, repository_url: &str, api_key: &str) -> Result<(), PckgError> {
    let canonical_url = canonical_repository_url(repository_url)?;
    let mut config = load_repositories_config(config_path)?;
    config.repositories.insert(canonical_url, RepositoryAuthConfig { api_key: api_key.to_string() });

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
                let default_repository = canonical_repository_url("https://pckg.beskid-lang.org")?;
                repositories.insert(default_repository, RepositoryAuthConfig { api_key: legacy_key });
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
