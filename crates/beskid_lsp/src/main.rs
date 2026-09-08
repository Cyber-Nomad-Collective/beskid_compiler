#[tokio::main]
async fn main() {
    let mut arguments = std::env::args_os().skip(1);
    if arguments.next().as_deref() == Some(std::ffi::OsStr::new("--version")) && arguments.next().is_none() {
        println!("beskid_lsp {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if let Err(error) = beskid_lsp::run_stdio_server().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
