#[tokio::main]
async fn main() {
    if let Err(error) = beskid_lsp::run_stdio_server().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
