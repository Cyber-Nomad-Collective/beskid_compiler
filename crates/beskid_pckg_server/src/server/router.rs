use std::sync::Arc;

use axum::{
    Json, Router,
    http::StatusCode,
    response::IntoResponse,
    routing::{any, delete, get},
};
use beskid_pckg_artifacts::LocalFileArtifactStore;
use beskid_pckg_contract::{ApiErrorResponse, HealthResponse};
use beskid_pckg_store::SqlxPackageRepository;
use sqlx::postgres::PgPoolOptions;
use tower_http::services::{ServeDir, ServeFile};

use super::auth::read_session;
use super::backend_memory::PackageBackend;
use super::config::{PckgServerConfig, ServerStartupError};
use super::model::AppState;
use crate::{
    admin_routes, api_key_routes, artifact_routes, embed, operations_routes, packages, workspace_review_routes,
};

/// Builds the in-memory server used by unit tests and explicitly database-free
/// local runs. Production callers with `PCKG_DATABASE_URL` must use
/// [`router_from_config`] or [`serve`] so a failed database migration can never
/// silently fall back to volatile data.
pub fn router(config: PckgServerConfig) -> Router {
    assert!(config.database_url.is_none(), "PCKG_DATABASE_URL is configured; use router_from_config or serve");
    router_with_backend(config, PackageBackend::in_memory())
}

/// Connects the configured PostgreSQL repository, applies the pckg-owned
/// idempotent migrations, then constructs the HTTP router. Roles are projected
/// from Authelia groups, so no bootstrap subject is seeded by the registry.
pub async fn router_from_config(config: PckgServerConfig) -> Result<Router, ServerStartupError> {
    let Some(database_url) = config.database_url.clone() else {
        return Ok(router_with_backend(config, PackageBackend::in_memory()));
    };
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await
        .map_err(|error| ServerStartupError(format!("cannot connect to PCKG_DATABASE_URL: {error}")))?;
    let repository = Arc::new(SqlxPackageRepository::new(pool.clone()));
    repository
        .migrate()
        .await
        .map_err(|error| ServerStartupError(format!("pckg registry migration failed: {error:?}")))?;
    repository
        .migrate_api_keys()
        .await
        .map_err(|error| ServerStartupError(format!("pckg API-key migration failed: {error:?}")))?;
    repository
        .migrate_administration()
        .await
        .map_err(|error| ServerStartupError(format!("pckg administration migration failed: {error:?}")))?;
    Ok(router_with_backend(config, PackageBackend::Sqlx(repository)))
}

fn router_with_backend(config: PckgServerConfig, packages: PackageBackend) -> Router {
    let web_root = config.web_root.clone();
    let index = web_root.join("index.html");
    let artifacts = LocalFileArtifactStore::new(&config.artifact_root)
        .expect("pckg artifact root is creatable and canonicalizable");
    let moderation_repository = match &packages {
        PackageBackend::Sqlx(repository) => Some(repository.clone()),
        PackageBackend::InMemory(_) => None,
    };
    let operations = match &packages {
        PackageBackend::Sqlx(repository) => operations_routes::OperationsState::sqlx(repository.clone()),
        PackageBackend::InMemory(_) => operations_routes::OperationsState::in_memory(),
    };
    let api_keys = moderation_repository.clone();
    Router::new()
        .route("/health", get(health))
        .route("/health/live", get(health))
        .route("/health/ready", get(health))
        .route("/api/packages", get(packages::list_packages).post(packages::upsert_package))
        .route("/api/search", get(packages::search_packages))
        .route("/api/embed/badge.svg", get(embed::badge))
        .route("/api/embed/card", get(embed::card))
        .route("/api/publishers", get(packages::list_publishers))
        .route("/api/publishers/{subject}/packages", get(packages::publisher_packages))
        .route(
            "/api/packages/{name}/community-reviews",
            get(packages::list_community_reviews).post(packages::create_community_review),
        )
        .route("/api/packages/{idOrName}", get(packages::package_detail).delete(packages::delete_package))
        .route("/api/packages/{name}/versions", get(packages::list_versions).post(packages::publish_version))
        .route("/api/packages/{name}/versions/{version}/yank", axum::routing::post(packages::yank_version))
        .route("/api/packages/{name}/versions/{version}/unyank", axum::routing::post(packages::unyank_version))
        .route("/api/packages/{name}/versions/{version}/artifact", axum::routing::post(packages::upload_artifact))
        .route("/api/packages/{name}/versions/{version}/download", get(packages::download_artifact))
        .route("/api/packages/{name}/versions/{version}/readme", get(artifact_routes::readme))
        .route("/api/packages/{name}/versions/{version}/docs", get(artifact_routes::list_docs))
        .route("/api/packages/{name}/versions/{version}/docs/file", get(artifact_routes::read_doc))
        .route("/api/packages/{name}/versions/{version}/docs/structured", get(artifact_routes::structured_docs))
        .route("/api/packages/{name}/versions/{version}/source/tree", get(artifact_routes::source_tree))
        .route("/api/packages/{name}/versions/{version}/source/file", get(artifact_routes::read_source))
        .route("/api/workspaces/publish", axum::routing::post(workspace_review_routes::publish_workspace))
        .route(
            "/api/packages/{name}/review-requests",
            axum::routing::post(workspace_review_routes::submit_review_request),
        )
        .route("/api/packages/reviews", get(workspace_review_routes::list_review_queue))
        .route("/api/packages/reviews/{review_id}/actions", axum::routing::post(workspace_review_routes::review_action))
        .route("/api/auth/session", get(read_session))
        .route("/api/api-keys", get(api_key_routes::list_api_keys).post(api_key_routes::create_api_key))
        .route("/api/api-keys/{id}", delete(api_key_routes::revoke_api_key))
        .route("/api/admin/users", get(admin_routes::list_users))
        .route("/api/admin/users/{subject}", axum::routing::patch(admin_routes::update_user))
        .route("/api/admin/permissions", get(admin_routes::list_permissions).post(admin_routes::grant_permission))
        .route(
            "/api/admin/publishers/{subject}/verification",
            axum::routing::put(admin_routes::set_publisher_verification),
        )
        .route(
            "/api/admin/packages/{name}/versions/{version}/review",
            axum::routing::post(admin_routes::review_package_version),
        )
        .merge(operations_routes::router())
        .route("/api", any(api_not_found))
        .route("/api/{*path}", any(api_not_found))
        .with_state(AppState {
            auth: config.auth,
            packages,
            artifacts: Arc::new(artifacts),
            api_keys,
            reviews: workspace_review_routes::ReviewQueueState::default(),
            operations,
        })
        .fallback_service(ServeDir::new(web_root).fallback(ServeFile::new(index)))
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse::ok())
}

async fn api_not_found() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, Json(ApiErrorResponse::new("API endpoint not found")))
}
