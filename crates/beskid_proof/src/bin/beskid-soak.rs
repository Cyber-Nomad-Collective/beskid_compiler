use beskid_proof::soak::{SoakConfig, run};

fn main() {
    let (config, smoke) = match SoakConfig::from_args(std::env::args().skip(1)) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };
    match run(&config, smoke) {
        Ok(report) => {
            print!("{}", report.render());
            if let Some(path) = &config.report
                && let Err(error) = std::fs::write(path, serde_json::to_string_pretty(&report.to_json()).unwrap_or_default())
            {
                eprintln!("cannot write report {}: {error}", path.display());
            }
            std::process::exit(if report.passed() { 0 } else { 1 });
        }
        Err(message) => {
            eprintln!("soak setup failed: {message}");
            std::process::exit(2);
        }
    }
}
