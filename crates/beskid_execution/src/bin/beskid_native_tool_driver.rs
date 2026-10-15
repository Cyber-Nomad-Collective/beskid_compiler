fn main() {
    let code = match beskid_execution::run_compiler_driver(std::env::args_os().skip(1).collect()) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("beskid native compiler driver: {error}");
            101
        }
    };
    std::process::exit(code);
}
