//! Axum composition for the pckg compatibility server.

mod admin_routes;
mod api_key_routes;
mod artifact_routes;
mod embed;
mod operations_routes;
mod packages;
mod server;
mod workspace_review_routes;

pub(crate) use self::server::{
    AppState, authenticated_principal, authenticated_subject, format_timestamp, now_unix_seconds, unauthorized_response,
};
pub use self::server::{PckgServerConfig, ServerStartupError, router, router_from_config, serve};
