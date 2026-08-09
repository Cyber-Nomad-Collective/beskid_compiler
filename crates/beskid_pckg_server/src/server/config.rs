use std::{
    env, fmt,
    net::SocketAddr,
    path::{Path as FilePath, PathBuf},
    sync::Arc,
};

use beskid_pckg_auth::{AuthHubHandoffVerifier, Hs256AuthHubHandoffVerifier};

#[derive(Clone)]
pub struct PckgServerConfig {
    pub bind_address: SocketAddr,
    pub(crate) web_root: PathBuf,
    pub(crate) artifact_root: PathBuf,
    pub(crate) database_url: Option<String>,
    pub(crate) admin_bootstrap_subject: Option<String>,
    pub(crate) auth: Option<AuthConfig>,
}

#[derive(Clone)]
pub(crate) struct AuthConfig {
    pub(crate) handoff_verifier: Arc<dyn AuthHubHandoffVerifier>,
    pub(crate) session_secret: String,
    pub(crate) secure_cookies: bool,
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
            admin_bootstrap_subject: None,
            auth: None,
        }
    }
}

impl PckgServerConfig {
    pub fn from_environment() -> Result<Self, beskid_pckg_auth::AuthError> {
        let service_token =
            env::var("PCKG_AUTH_HUB_SERVICE_TOKEN").map_err(|_| beskid_pckg_auth::AuthError::MissingConfiguration)?;
        let session_secret =
            env::var("PCKG_SESSION_SECRET").map_err(|_| beskid_pckg_auth::AuthError::MissingConfiguration)?;
        Ok(Self::with_auth_secrets(service_token, session_secret)
            .with_secure_cookies(env::var("PCKG_COOKIE_SECURE").map(|value| value != "false").unwrap_or(true))
            .with_web_root(env::var_os("PCKG_WEB_ROOT").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/app/web")))
            .with_artifact_root(
                env::var_os("PCKG_ARTIFACT_ROOT").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/app/artifacts")),
            )
            .with_database_url(env::var("PCKG_DATABASE_URL").ok())
            .with_admin_bootstrap_subject(env::var("PCKG_ADMIN_BOOTSTRAP_SUBJECT").ok()))
    }

    pub fn with_auth_secrets(service_token: impl Into<String>, session_secret: impl Into<String>) -> Self {
        let session_secret = session_secret.into();
        let handoff_verifier =
            Hs256AuthHubHandoffVerifier::new(service_token).expect("explicit auth hub service token is non-empty");
        Self {
            auth: Some(AuthConfig {
                handoff_verifier: Arc::new(handoff_verifier),
                session_secret,
                secure_cookies: false,
            }),
            ..Self::default()
        }
    }

    pub fn with_web_root(mut self, web_root: impl AsRef<FilePath>) -> Self {
        self.web_root = web_root.as_ref().to_path_buf();
        self
    }

    /// Enables the `Secure` attribute for browser sessions. Environment-backed
    /// production configuration enables this by default; tests and HTTP-only
    /// local development can opt out explicitly.
    pub fn with_secure_cookies(mut self, secure_cookies: bool) -> Self {
        if let Some(auth) = &mut self.auth {
            auth.secure_cookies = secure_cookies;
        }
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

    /// Explicit, one-time deployment bootstrap. This is ignored once any
    /// admin role exists; it never defaults to a user or GitHub login.
    pub fn with_admin_bootstrap_subject(mut self, subject: Option<String>) -> Self {
        self.admin_bootstrap_subject = subject.filter(|value| !value.trim().is_empty());
        self
    }
}
