use super::config::PckgServerConfig;
use super::router::router_from_config;

pub async fn serve(config: PckgServerConfig) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(config.bind_address).await?;
    let router = router_from_config(config).await.map_err(std::io::Error::other)?;
    axum::serve(listener, router).await
}
