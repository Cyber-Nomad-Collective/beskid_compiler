//! CLI logging defaults for `env_logger`.
//!
//! By default, Cranelift JIT/codegen backends are quiet at `info` so `beskid test` and
//! `beskid run` stay readable. Enable backend traces with:
//!
//! - `beskid --log-cranelift …` (or any subcommand)
//! - `BESKID_LOG_CRANELIFT=1` in the environment
//! - `RUST_LOG=info,cranelift_jit=info,cranelift_codegen=info` (full control via `RUST_LOG`)

const CRANELIFT_QUIET: &str = "cranelift_jit=warn,cranelift_codegen=warn,cranelift_frontend=warn,cranelift_module=warn,cranelift_native=warn,cranelift_object=warn";

/// Initialize `env_logger` after CLI parsing.
///
/// When `log_cranelift` is false and `RUST_LOG` is unset, Cranelift crates default to `warn`
/// while other modules remain at `info`. When `log_cranelift` is true, the default filter is
/// plain `info` (including Cranelift backends). An explicit `RUST_LOG` always wins.
pub fn init(log_cranelift: bool) {
    let default = if log_cranelift {
        "info".to_string()
    } else {
        format!("info,{CRANELIFT_QUIET}")
    };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(&default)).init();
}
