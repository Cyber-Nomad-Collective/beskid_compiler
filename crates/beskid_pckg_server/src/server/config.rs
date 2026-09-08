use std::{
    env, fmt,
    net::SocketAddr,
    path::{Path as FilePath, PathBuf},
    sync::Arc,
};

use beskid_pckg_auth::AuthMode;
use beskid_pckg_store::{AsyncApiKeyRepository, validate_release_publisher_key_sha256};

#[derive(Clone)]
pub struct PckgServerConfig {
    pub bind_address: SocketAddr,
    pub(crate) web_root: PathBuf,
    pub(crate) artifact_root: PathBuf,
    pub(crate) database_url: Option<String>,
    pub(crate) auth: Option<AuthConfig>,
    pub(crate) api_key_repository: Option<Arc<dyn AsyncApiKeyRepository>>,
    pub(crate) release_publisher_key_sha256: Option<String>,
}

/// Authentication configuration. pckg is a resource server that trusts
/// Authelia's forward-auth session; it never authenticates users itself.
#[derive(Clone)]
pub(crate) struct AuthConfig {
    pub(crate) mode: AuthMode,
    /// Authelia group that maps to the pckg `SuperAdmin` role.
    pub(crate) admin_group: String,
    /// Authelia group that maps to the pckg `Moderator` role.
    pub(crate) moderator_group: String,
    /// Subject trusted in `SHELL_AUTH_MODE=mock` (local dev only).
    pub(crate) mock_subject: String,
    /// Groups trusted for the mock subject.
    pub(crate) mock_groups: Vec<String>,
}

#[derive(Debug)]
pub struct ServerStartupError(pub(crate) String);

impl fmt::Display for ServerStartupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ServerStartupError {}

impl Default for PckgServerConfig {
    fn default() -> Self {
        Self {
            bind_address: SocketAddr::from(([0, 0, 0, 0], 8082)),
            web_root: PathBuf::from("/app/web"),
            artifact_root: env::temp_dir().join("beskid-pckg-artifacts"),
            database_url: None,
            auth: None,
            api_key_repository: None,
            release_publisher_key_sha256: None,
        }
    }
}

impl PckgServerConfig {
    /// Builds configuration from process environment. `SHELL_AUTH_MODE`
    /// selects the authentication mode (`mock` for local dev, `authelia` for
    /// production behind Authelia forward-auth). When unset, no auth is
    /// configured, browser-session routes are rejected while a configured
    /// publisher API-key repository can still authenticate publication routes.
    pub fn from_environment() -> Result<Self, ServerStartupError> {
        let mut config = Self::default()
            .with_web_root(env::var_os("PCKG_WEB_ROOT").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/app/web")))
            .with_artifact_root(
                env::var_os("PCKG_ARTIFACT_ROOT").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/app/artifacts")),
            )
            .with_database_url(env::var("PCKG_DATABASE_URL").ok())
            .with_bind_address(env::var("PCKG_BIND_ADDRESS").ok())
            .with_release_publisher_key_sha256(env::var("PCKG_RELEASE_PUBLISHER_KEY_SHA256").ok())?;
        if let Ok(mode) = env::var("SHELL_AUTH_MODE") {
            let mode = AuthMode::parse(&mode)
                .map_err(|error| ServerStartupError(format!("invalid SHELL_AUTH_MODE: {error}")))?;
            config = config.with_auth(AuthConfig {
                mode,
                admin_group: env::var("SHELL_ADMIN_GROUP").unwrap_or_else(|_| "pckg-admins".to_owned()),
                moderator_group: env::var("SHELL_MODERATOR_GROUP").unwrap_or_else(|_| "pckg-moderators".to_owned()),
                mock_subject: env::var("SHELL_MOCK_SUBJECT").unwrap_or_else(|_| "local-admin".to_owned()),
                mock_groups: env::var("SHELL_MOCK_GROUPS")
                    .unwrap_or_else(|_| "pckg-admins".to_owned())
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
                    .collect(),
            });
        }
        Ok(config)
    }

    pub fn with_bind_address(mut self, address: Option<String>) -> Self {
        if let Some(address) = address.filter(|value| !value.trim().is_empty()) {
            self.bind_address = address.parse().expect("PCKG_BIND_ADDRESS is a valid socket address");
        }
        self
    }

    pub fn with_web_root(mut self, web_root: impl AsRef<FilePath>) -> Self {
        self.web_root = web_root.as_ref().to_path_buf();
        self
    }

    /// Configures the filesystem root used by the single-node artifact store.
    ///
    /// Production object storage must preserve the same `storage_key` and
    /// checksum behavior before replacing this adapter.
    pub fn with_artifact_root(mut self, artifact_root: impl AsRef<FilePath>) -> Self {
        self.artifact_root = artifact_root.as_ref().to_path_buf();
        self
    }

    /// Selects PostgreSQL persistence for the runtime. The connection and the
    /// registry-owned idempotent migrations are applied by
    /// [`router_from_config`] / [`serve`], never by individual requests.
    pub fn with_database_url(mut self, database_url: Option<String>) -> Self {
        self.database_url = database_url.filter(|value| !value.trim().is_empty());
        self
    }

    /// Supplies the pckg-owned key verifier used by CLI publication routes.
    /// Production receives this from the configured PostgreSQL repository;
    /// this injection seam is for explicit embedded/test deployments.
    pub fn with_api_key_repository(mut self, repository: Arc<dyn AsyncApiKeyRepository>) -> Self {
        self.api_key_repository = Some(repository);
        self
    }

    /// Supplies the SHA-256 digest for the deterministic release automation
    /// key. Production must never pass the raw `bpk_...` token to the server.
    pub fn with_release_publisher_key_sha256(
        mut self,
        token_sha256: Option<String>,
    ) -> Result<Self, ServerStartupError> {
        if let Some(token_sha256) = token_sha256 {
            validate_release_publisher_key_sha256(&token_sha256).map_err(|_| {
                ServerStartupError(
                    "PCKG_RELEASE_PUBLISHER_KEY_SHA256 must be exactly 64 lowercase hexadecimal characters".to_owned(),
                )
            })?;
            self.release_publisher_key_sha256 = Some(token_sha256);
        }
        Ok(self)
    }

    pub(crate) fn with_auth(mut self, auth: AuthConfig) -> Self {
        self.auth = Some(auth);
        self
    }

    /// Configures Authelia forward-auth mode with the default admin/moderator
    /// group names. Intended for tests and local development; production uses
    /// [`Self::from_environment`].
    pub fn with_authelia_auth(self) -> Self {
        self.with_auth(AuthConfig {
            mode: AuthMode::Authelia,
            admin_group: "pckg-admins".to_owned(),
            moderator_group: "pckg-moderators".to_owned(),
            mock_subject: "local-admin".to_owned(),
            mock_groups: vec!["pckg-admins".to_owned()],
        })
    }

    /// Configures Authentik proxy-outpost mode with the default role groups.
    pub fn with_authentik_auth(self) -> Self {
        self.with_auth(AuthConfig {
            mode: AuthMode::Authentik,
            admin_group: "pckg-admins".to_owned(),
            moderator_group: "pckg-moderators".to_owned(),
            mock_subject: "local-admin".to_owned(),
            mock_groups: vec!["pckg-admins".to_owned()],
        })
    }

    /// Configures mock mode with a single dev admin subject. Intended for
    /// tests and local development without an Authelia instance.
    pub fn with_mock_auth(self) -> Self {
        self.with_auth(AuthConfig {
            mode: AuthMode::Mock,
            admin_group: "pckg-admins".to_owned(),
            moderator_group: "pckg-moderators".to_owned(),
            mock_subject: "local-admin".to_owned(),
            mock_groups: vec!["pckg-admins".to_owned()],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::PckgServerConfig;

    #[test]
    fn release_publisher_key_configuration_rejects_raw_or_malformed_credentials() {
        assert!(PckgServerConfig::default().with_release_publisher_key_sha256(Some("a".repeat(64))).is_ok());
        assert!(
            PckgServerConfig::default()
                .with_release_publisher_key_sha256(Some(format!("bpk_{}", "a".repeat(64))))
                .is_err()
        );
        assert!(PckgServerConfig::default().with_release_publisher_key_sha256(Some("A".repeat(64))).is_err());
    }
}
