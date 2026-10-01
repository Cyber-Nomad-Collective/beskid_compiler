//! Subscriber setup runs in child processes because tracing initialization is global.

#[test]
fn cli_and_lsp_initialization_emit_ordinary_line_logs() {
    for scope in ["cli", "lsp"] {
        let output = std::process::Command::new(std::env::current_exe().expect("test executable"))
            .args(["--exact", "initialization_fixture", "--ignored", "--nocapture"])
            .env("BESKID_TELEMETRY_TEST_SCOPE", scope)
            .env_remove("RUST_LOG")
            .env("OTEL_SDK_DISABLED", "true")
            .output()
            .expect("subscriber child process");
        assert!(output.status.success(), "{scope}: {}", String::from_utf8_lossy(&output.stderr));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("tracing subscriber initialized"), "{scope}: subscriber never initialized: {stderr}");
        assert!(
            stderr.contains("ordinary line logging regression"),
            "{scope}: logging stopped after initialization: {stderr}"
        );
        assert!(!output.stderr.contains(&27), "{scope}: terminal control bytes in logs: {stderr:?}");
        assert!(stderr.ends_with('\n'));
    }
}

#[test]
#[ignore = "subprocess-only fixture for global subscriber initialization"]
fn initialization_fixture() {
    let scope = std::env::var("BESKID_TELEMETRY_TEST_SCOPE").expect("fixture scope");
    let options = match scope.as_str() {
        "cli" => beskid_telemetry::InitOptions::cli(false),
        "lsp" => beskid_telemetry::InitOptions::lsp(),
        other => panic!("unknown fixture scope {other}"),
    };
    beskid_telemetry::init(options);
    tracing::info!("ordinary line logging regression");
}
