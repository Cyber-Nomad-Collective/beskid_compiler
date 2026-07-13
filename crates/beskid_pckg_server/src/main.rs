use beskid_pckg_server::{PckgServerConfig, serve};

#[tokio::main]
async fn main() {
    let config = PckgServerConfig::from_environment().unwrap_or_else(|error| {
        eprintln!("pckg server configuration error: {error}");
        std::process::exit(1);
    });

    if let Err(error) = serve(config).await {
        eprintln!("pckg server error: {error}");
        std::process::exit(1);
    }
}
