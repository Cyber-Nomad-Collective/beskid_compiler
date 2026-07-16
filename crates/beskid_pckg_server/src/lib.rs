//! Axum composition for the pckg compatibility server.

use std::{
    collections::{BTreeMap, BTreeSet},
    env, fmt,
    net::SocketAddr,
    path::{Path as FilePath, PathBuf},
    sync::Arc,
};

use axum::{
    Json, Router,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Redirect},
    routing::{any, delete, get},
};
use beskid_pckg_artifacts::LocalFileArtifactStore;
use beskid_pckg_auth::{
    AuthHubHandoffVerifier, AuthHubIdentity, HandoffRequest, Hs256AuthHubHandoffVerifier,
    issue_pckg_session, verify_pckg_session,
};
use beskid_pckg_contract::{ApiErrorResponse, HealthResponse, SessionResponse};
use beskid_pckg_store::{
    AsyncPackageRepository, InMemoryPackageRepository, NewPackage, Package, PackageRepository,
    PackageVersion, PublishOutcome, PublishVersion, SqlxCommunityRepository, SqlxPackageRepository,
    StoreError,
};
use serde::Deserialize;
use sqlx::Row;
use sqlx::postgres::PgPoolOptions;
use tower_http::services::{ServeDir, ServeFile};

mod api_key_routes;
mod artifact_routes;
mod community_routes;
mod packages;

#[derive(Clone)]
pub struct PckgServerConfig {
    pub bind_address: SocketAddr,
    web_root: PathBuf,
    artifact_root: PathBuf,
    database_url: Option<String>,
    auth: Option<AuthConfig>,
}

#[derive(Clone)]
struct AuthConfig {
    handoff_verifier: Arc<dyn AuthHubHandoffVerifier>,
    session_secret: String,
    secure_cookies: bool,
}

#[derive(Clone)]
struct AppState {
    auth: Option<AuthConfig>,
    packages: PackageBackend,
    artifacts: Arc<LocalFileArtifactStore>,
    api_keys: Option<Arc<SqlxPackageRepository>>,
}

/// Storage is selected exactly once during startup. In-memory storage remains
/// intentionally available for isolated HTTP tests and local UI work without a
/// database; a configured database always goes through the SQLx boundary.
#[derive(Clone)]
pub(crate) enum PackageBackend {
    InMemory(Arc<InMemoryPackageBackend>),
    Sqlx(Arc<SqlxPackageRepository>),
}

/// Server-owned indexes make read enumeration available to the intentionally
/// minimal in-memory repository. PostgreSQL reads query the canonical tables
/// directly, so this is only a deterministic test/local adapter.
#[derive(Default)]
struct InMemoryPackageBackend {
    repository: std::sync::Mutex<InMemoryPackageRepository>,
    package_names: std::sync::Mutex<BTreeSet<String>>,
    versions_by_package: std::sync::Mutex<BTreeMap<String, BTreeSet<String>>>,
}

impl PackageBackend {
    fn in_memory() -> Self {
        Self::InMemory(Arc::new(InMemoryPackageBackend::default()))
    }

    async fn create_package(&self, request: NewPackage) -> Result<Package, StoreError> {
        match self {
            Self::InMemory(repository) => {
                let package = repository
                    .repository
                    .lock()
                    .expect("package repository mutex is not poisoned")
                    .create_package(request)?;
                repository
                    .package_names
                    .lock()
                    .expect("package catalog mutex is not poisoned")
                    .insert(package.name.clone());
                Ok(package)
            }
            Self::Sqlx(repository) => repository.create_package(request).await,
        }
    }

    async fn find_package(&self, name: &str) -> Result<Option<Package>, StoreError> {
        match self {
            Self::InMemory(repository) => Ok(repository
                .repository
                .lock()
                .expect("package repository mutex is not poisoned")
                .find_package(name)
                .cloned()),
            Self::Sqlx(repository) => repository.find_package(name).await,
        }
    }

    async fn find_package_by_id(&self, id: &str) -> Result<Option<Package>, StoreError> {
        match self {
            Self::InMemory(repository) => {
                let names = repository
                    .package_names
                    .lock()
                    .expect("package catalog mutex is not poisoned")
                    .clone();
                let repository = repository
                    .repository
                    .lock()
                    .expect("package repository mutex is not poisoned");
                Ok(names.into_iter().find_map(|name| {
                    repository
                        .find_package(&name)
                        .filter(|package| package.id == id)
                        .cloned()
                }))
            }
            Self::Sqlx(repository) => sqlx_find_package_by_id(repository, id).await,
        }
    }

    async fn list_packages(&self, limit: i64, offset: i64) -> Result<Vec<Package>, StoreError> {
        match self {
            Self::InMemory(repository) => {
                let names = repository
                    .package_names
                    .lock()
                    .expect("package catalog mutex is not poisoned")
                    .clone();
                let repository = repository
                    .repository
                    .lock()
                    .expect("package repository mutex is not poisoned");
                let mut packages = names
                    .into_iter()
                    .filter_map(|name| repository.find_package(&name).cloned())
                    .collect::<Vec<_>>();
                packages.sort_by(|left, right| {
                    right
                        .updated_at_unix_seconds
                        .cmp(&left.updated_at_unix_seconds)
                        .then_with(|| left.name.cmp(&right.name))
                });
                Ok(packages
                    .into_iter()
                    .skip(offset as usize)
                    .take(limit as usize)
                    .collect())
            }
            Self::Sqlx(repository) => sqlx_list_packages(repository, limit, offset).await,
        }
    }

    async fn list_versions(&self, package_id: &str) -> Result<Vec<PackageVersion>, StoreError> {
        match self {
            Self::InMemory(repository) => {
                let versions = repository
                    .versions_by_package
                    .lock()
                    .expect("version catalog mutex is not poisoned")
                    .get(package_id)
                    .cloned()
                    .unwrap_or_default();
                let repository = repository
                    .repository
                    .lock()
                    .expect("package repository mutex is not poisoned");
                let mut versions = versions
                    .into_iter()
                    .filter_map(|version| repository.find_version(package_id, &version).cloned())
                    .collect::<Vec<_>>();
                versions.sort_by(|left, right| {
                    right
                        .published_at_unix_seconds
                        .cmp(&left.published_at_unix_seconds)
                        .then_with(|| right.version.cmp(&left.version))
                });
                Ok(versions)
            }
            Self::Sqlx(repository) => sqlx_list_versions(repository, package_id).await,
        }
    }

    async fn publish_version(&self, request: PublishVersion) -> Result<PublishOutcome, StoreError> {
        match self {
            Self::InMemory(repository) => {
                let outcome = repository
                    .repository
                    .lock()
                    .expect("package repository mutex is not poisoned")
                    .publish_version(request)?;
                let version = match &outcome {
                    PublishOutcome::Created(version) | PublishOutcome::AlreadyExists(version) => {
                        version
                    }
                };
                repository
                    .versions_by_package
                    .lock()
                    .expect("version catalog mutex is not poisoned")
                    .entry(version.package_id.clone())
                    .or_default()
                    .insert(version.version.clone());
                Ok(outcome)
            }
            Self::Sqlx(repository) => repository.publish_version(request).await,
        }
    }

    async fn find_version(
        &self,
        package_id: &str,
        version: &str,
    ) -> Result<Option<PackageVersion>, StoreError> {
        match self {
            Self::InMemory(repository) => Ok(repository
                .repository
                .lock()
                .expect("package repository mutex is not poisoned")
                .find_version(package_id, version)
                .cloned()),
            Self::Sqlx(repository) => repository.find_version(package_id, version).await,
        }
    }

    async fn set_yanked(
        &self,
        package_id: &str,
        version: &str,
        yanked: bool,
        now_unix_seconds: i64,
    ) -> Result<PackageVersion, StoreError> {
        match self {
            Self::InMemory(repository) => repository
                .repository
                .lock()
                .expect("package repository mutex is not poisoned")
                .set_yanked(package_id, version, yanked, now_unix_seconds),
            Self::Sqlx(repository) => {
                repository
                    .set_yanked(package_id, version, yanked, now_unix_seconds)
                    .await
            }
        }
    }
}

fn row_package(row: sqlx::postgres::PgRow) -> Result<Package, StoreError> {
    Ok(Package {
        id: row
            .try_get("id")
            .map_err(|error| StoreError::Database(error.to_string()))?,
        name: row
            .try_get("name")
            .map_err(|error| StoreError::Database(error.to_string()))?,
        owner_subject: row
            .try_get("owner_subject")
            .map_err(|error| StoreError::Database(error.to_string()))?,
        is_public: row
            .try_get("is_public")
            .map_err(|error| StoreError::Database(error.to_string()))?,
        created_at_unix_seconds: row
            .try_get("created_at")
            .map_err(|error| StoreError::Database(error.to_string()))?,
        updated_at_unix_seconds: row
            .try_get("updated_at")
            .map_err(|error| StoreError::Database(error.to_string()))?,
    })
}

fn row_version(row: sqlx::postgres::PgRow) -> Result<PackageVersion, StoreError> {
    let size_bytes: i64 = row
        .try_get("size_bytes")
        .map_err(|error| StoreError::Database(error.to_string()))?;
    Ok(PackageVersion {
        id: row
            .try_get("id")
            .map_err(|error| StoreError::Database(error.to_string()))?,
        package_id: row
            .try_get("package_id")
            .map_err(|error| StoreError::Database(error.to_string()))?,
        version: row
            .try_get("version")
            .map_err(|error| StoreError::Database(error.to_string()))?,
        checksum_sha256: row
            .try_get("checksum_sha256")
            .map_err(|error| StoreError::Database(error.to_string()))?,
        storage_key: row
            .try_get("storage_key")
            .map_err(|error| StoreError::Database(error.to_string()))?,
        size_bytes: size_bytes
            .try_into()
            .map_err(|_| StoreError::InvalidIdentifier)?,
        is_yanked: row
            .try_get("is_yanked")
            .map_err(|error| StoreError::Database(error.to_string()))?,
        published_at_unix_seconds: row
            .try_get("published_at")
            .map_err(|error| StoreError::Database(error.to_string()))?,
        yanked_at_unix_seconds: row
            .try_get("yanked_at")
            .map_err(|error| StoreError::Database(error.to_string()))?,
    })
}

const PACKAGE_SELECT: &str = "SELECT id::text AS id, name, owner_subject, is_public, EXTRACT(EPOCH FROM created_at_utc)::bigint AS created_at, EXTRACT(EPOCH FROM updated_at_utc)::bigint AS updated_at FROM pckg_packages";
const VERSION_SELECT: &str = "SELECT id::text AS id, package_id::text AS package_id, version, checksum_sha256, storage_key, size_bytes, is_yanked, EXTRACT(EPOCH FROM published_at_utc)::bigint AS published_at, EXTRACT(EPOCH FROM yanked_at_utc)::bigint AS yanked_at FROM pckg_package_versions";

async fn sqlx_find_package_by_id(
    repository: &SqlxPackageRepository,
    id: &str,
) -> Result<Option<Package>, StoreError> {
    let query = format!("{PACKAGE_SELECT} WHERE id::text = $1");
    sqlx::query(&query)
        .bind(id)
        .fetch_optional(repository.pool())
        .await
        .map_err(|error| StoreError::Database(error.to_string()))?
        .map(row_package)
        .transpose()
}

async fn sqlx_list_packages(
    repository: &SqlxPackageRepository,
    limit: i64,
    offset: i64,
) -> Result<Vec<Package>, StoreError> {
    let query =
        format!("{PACKAGE_SELECT} ORDER BY updated_at_utc DESC, name ASC LIMIT $1 OFFSET $2");
    sqlx::query(&query)
        .bind(limit)
        .bind(offset)
        .fetch_all(repository.pool())
        .await
        .map_err(|error| StoreError::Database(error.to_string()))?
        .into_iter()
        .map(row_package)
        .collect()
}

async fn sqlx_list_versions(
    repository: &SqlxPackageRepository,
    package_id: &str,
) -> Result<Vec<PackageVersion>, StoreError> {
    let query = format!(
        "{VERSION_SELECT} WHERE package_id::text = $1 ORDER BY published_at_utc DESC, version DESC"
    );
    sqlx::query(&query)
        .bind(package_id)
        .fetch_all(repository.pool())
        .await
        .map_err(|error| StoreError::Database(error.to_string()))?
        .into_iter()
        .map(row_version)
        .collect()
}

#[derive(Debug)]
pub struct ServerStartupError(String);

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
        }
    }
}

impl PckgServerConfig {
    pub fn from_environment() -> Result<Self, beskid_pckg_auth::AuthError> {
        let service_token = env::var("PCKG_AUTH_HUB_SERVICE_TOKEN")
            .map_err(|_| beskid_pckg_auth::AuthError::MissingConfiguration)?;
        let session_secret = env::var("PCKG_SESSION_SECRET")
            .map_err(|_| beskid_pckg_auth::AuthError::MissingConfiguration)?;
        Ok(Self::with_auth_secrets(service_token, session_secret)
            .with_secure_cookies(
                env::var("PCKG_COOKIE_SECURE")
                    .map(|value| value != "false")
                    .unwrap_or(true),
            )
            .with_web_root(
                env::var_os("PCKG_WEB_ROOT")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("/app/web")),
            )
            .with_artifact_root(
                env::var_os("PCKG_ARTIFACT_ROOT")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("/app/artifacts")),
            )
            .with_database_url(env::var("PCKG_DATABASE_URL").ok()))
    }

    pub fn with_auth_secrets(
        service_token: impl Into<String>,
        session_secret: impl Into<String>,
    ) -> Self {
        let session_secret = session_secret.into();
        let handoff_verifier = Hs256AuthHubHandoffVerifier::new(service_token)
            .expect("explicit auth hub service token is non-empty");
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
}

/// Builds the in-memory server used by unit tests and explicitly database-free
/// local runs. Production callers with `PCKG_DATABASE_URL` must use
/// [`router_from_config`] or [`serve`] so a failed database migration can never
/// silently fall back to volatile data.
pub fn router(config: PckgServerConfig) -> Router {
    assert!(
        config.database_url.is_none(),
        "PCKG_DATABASE_URL is configured; use router_from_config or serve"
    );
    router_with_backend(config, PackageBackend::in_memory(), None)
}

/// Connects the configured PostgreSQL repository, applies only pckg-owned
/// idempotent migrations, then constructs the HTTP router. The legacy Identity
/// import is deliberately not part of this boot path because it requires an
/// audited GitHub-subject mapping.
pub async fn router_from_config(config: PckgServerConfig) -> Result<Router, ServerStartupError> {
    let Some(database_url) = config.database_url.clone() else {
        return Ok(router_with_backend(
            config,
            PackageBackend::in_memory(),
            None,
        ));
    };
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await
        .map_err(|error| {
            ServerStartupError(format!("cannot connect to PCKG_DATABASE_URL: {error}"))
        })?;
    let repository = SqlxPackageRepository::new(pool.clone());
    repository.migrate().await.map_err(|error| {
        ServerStartupError(format!("pckg registry migration failed: {error:?}"))
    })?;
    repository
        .migrate_api_keys()
        .await
        .map_err(|error| ServerStartupError(format!("pckg API-key migration failed: {error:?}")))?;
    let community_repository = Arc::new(SqlxCommunityRepository::new(pool));
    community_repository.migrate().await.map_err(|error| {
        ServerStartupError(format!("pckg community migration failed: {error:?}"))
    })?;
    Ok(router_with_backend(
        config,
        PackageBackend::Sqlx(Arc::new(repository)),
        Some(community_repository),
    ))
}

fn router_with_backend(
    config: PckgServerConfig,
    packages: PackageBackend,
    community_repository: Option<Arc<SqlxCommunityRepository>>,
) -> Router {
    let web_root = config.web_root.clone();
    let index = web_root.join("index.html");
    let artifacts = LocalFileArtifactStore::new(&config.artifact_root)
        .expect("pckg artifact root is creatable and canonicalizable");
    let community_state = config
        .auth
        .as_ref()
        .map(|auth| match &community_repository {
            Some(repository) => community_routes::CommunityState::with_sqlx_session_secret(
                auth.session_secret.clone(),
                repository.clone(),
            ),
            None => {
                community_routes::CommunityState::with_session_secret(auth.session_secret.clone())
            }
        })
        .unwrap_or_default();
    let api_keys = match &packages {
        PackageBackend::Sqlx(repository) => Some(repository.clone()),
        PackageBackend::InMemory(_) => None,
    };
    Router::new()
        .route("/health", get(health))
        .route("/health/live", get(health))
        .route("/health/ready", get(health))
        .route(
            "/api/packages",
            get(packages::list_packages).post(packages::upsert_package),
        )
        .route("/api/search", get(packages::search_packages))
        .route("/api/packages/{idOrName}", get(packages::package_detail))
        .route(
            "/api/packages/{name}/versions",
            axum::routing::post(packages::publish_version),
        )
        .route(
            "/api/packages/{name}/versions/{version}/yank",
            axum::routing::post(packages::yank_version),
        )
        .route(
            "/api/packages/{name}/versions/{version}/unyank",
            axum::routing::post(packages::unyank_version),
        )
        .route(
            "/api/packages/{name}/versions/{version}/artifact",
            axum::routing::post(packages::upload_artifact),
        )
        .route(
            "/api/packages/{name}/versions/{version}/download",
            get(packages::download_artifact),
        )
        .route(
            "/api/packages/{name}/versions/{version}/readme",
            get(artifact_routes::readme),
        )
        .route(
            "/api/packages/{name}/versions/{version}/docs",
            get(artifact_routes::list_docs),
        )
        .route(
            "/api/packages/{name}/versions/{version}/docs/file",
            get(artifact_routes::read_doc),
        )
        .route(
            "/api/packages/{name}/versions/{version}/docs/structured",
            get(artifact_routes::structured_docs),
        )
        .route(
            "/api/packages/{name}/versions/{version}/source/tree",
            get(artifact_routes::source_tree),
        )
        .route(
            "/api/packages/{name}/versions/{version}/source/file",
            get(artifact_routes::read_source),
        )
        .route("/api/auth/hub-finish", get(auth_hub_finish))
        .route("/api/auth/session", get(read_session))
        .route(
            "/api/api-keys",
            get(api_key_routes::list_api_keys).post(api_key_routes::create_api_key),
        )
        .route("/api/api-keys/{id}", delete(api_key_routes::revoke_api_key))
        .nest_service("/api/community", community_routes::router(community_state))
        .route("/api", any(api_not_found))
        .route("/api/{*path}", any(api_not_found))
        .with_state(AppState {
            auth: config.auth,
            packages,
            artifacts: Arc::new(artifacts),
            api_keys,
        })
        .fallback_service(ServeDir::new(web_root).fallback(ServeFile::new(index)))
}

pub async fn serve(config: PckgServerConfig) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(config.bind_address).await?;
    let router = router_from_config(config)
        .await
        .map_err(std::io::Error::other)?;
    axum::serve(listener, router).await
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse::ok())
}

async fn api_not_found() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(ApiErrorResponse::new("API endpoint not found")),
    )
}

#[derive(Debug, Deserialize)]
struct AuthHubFinishQuery {
    handoff: Option<String>,
}

async fn auth_hub_finish(
    State(state): State<AppState>,
    Query(query): Query<AuthHubFinishQuery>,
) -> impl IntoResponse {
    let Some(handoff) = query.handoff.filter(|value| !value.trim().is_empty()) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiErrorResponse::new("handoff is required")),
        )
            .into_response();
    };
    let Some(auth) = state.auth else {
        return unauthorized_response();
    };
    let identity = match auth.handoff_verifier.verify(HandoffRequest {
        app: "pckg".to_owned(),
        handoff,
    }) {
        Ok(identity) => identity,
        Err(_) => return invalid_handoff_response(),
    };
    let session = match issue_pckg_session(&identity, &auth.session_secret) {
        Ok(session) => session,
        Err(_) => return invalid_handoff_response(),
    };
    let secure = if auth.secure_cookies { "; Secure" } else { "" };
    let cookie =
        format!("pckg_session={session}; HttpOnly; Path=/; SameSite=Lax; Max-Age=28800{secure}");
    let mut response = Redirect::to("/dashboard/packages/my").into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        cookie
            .parse()
            .expect("session cookie uses valid header characters"),
    );
    response
}

async fn read_session(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let Some(auth) = state.auth else {
        return unauthorized_response();
    };
    let Some(session) = session_cookie(&headers) else {
        return unauthorized_response();
    };
    match verify_pckg_session(session, &auth.session_secret) {
        Ok(identity) => Json(session_response(identity)).into_response(),
        Err(_) => unauthorized_response(),
    }
}

fn session_cookie(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix("pckg_session="))
}

fn session_response(identity: AuthHubIdentity) -> SessionResponse {
    SessionResponse {
        subject: identity.subject,
        github_login: identity.github_login,
        hub_session_id: identity.hub_session_id,
    }
}

pub(crate) fn authenticated_subject(state: &AppState, headers: &HeaderMap) -> Option<String> {
    let auth = state.auth.as_ref()?;
    let session = session_cookie(headers)?;
    verify_pckg_session(session, &auth.session_secret)
        .ok()
        .map(|identity| identity.subject)
}

pub(crate) fn unauthorized_response() -> axum::response::Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(ApiErrorResponse::new("authentication required")),
    )
        .into_response()
}

fn invalid_handoff_response() -> axum::response::Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(ApiErrorResponse::new("invalid handoff")),
    )
        .into_response()
}
