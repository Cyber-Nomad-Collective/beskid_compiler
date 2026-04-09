#[cfg(all(target_os = "linux", feature = "extern_dlopen"))]
use anyhow::Result;
#[cfg(all(target_os = "linux", feature = "extern_dlopen"))]
use beskid_codegen::services::lower_source;
#[cfg(all(target_os = "linux", feature = "extern_dlopen"))]
use beskid_engine::set_security_policies_for_tests;

#[test]
#[cfg(target_os = "linux")]
#[cfg(feature = "extern_dlopen")]
fn security_allow_deny_sequences() -> Result<()> {
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            set_security_policies_for_tests(None, None);
        }
    }
    let _g = Guard;

    let src = r#"
[Extern(Abi:"C", Library:"libc.so.6")]
pub contract C { i64 getpid(); }

pub i64 main() { return 0; }
"#;
    let lowered = lower_source(std::path::Path::new("<memory>"), src, false)?;

    // Allowed by allowlist
    set_security_policies_for_tests(Some("libc.so.6:getpid"), None);
    let mut engine = beskid_engine::Engine::new();
    engine
        .compile_artifact(&lowered.artifact)
        .expect("compile allowed");

    // Blocked by allowlist mismatch
    set_security_policies_for_tests(Some("libc.so.6:write"), None);
    let mut engine = beskid_engine::Engine::new();
    let err = engine
        .compile_artifact(&lowered.artifact)
        .expect_err("should be denied by allowlist");
    let msg = format!("{:?}", err);
    assert!(msg.contains("denied by allowlist"));

    // Blocked by denylist
    set_security_policies_for_tests(None, Some("libc.so.6:getpid"));
    let mut engine = beskid_engine::Engine::new();
    let err = engine
        .compile_artifact(&lowered.artifact)
        .expect_err("should be denied by denylist");
    let msg = format!("{:?}", err);
    assert!(msg.contains("denied by denylist"));

    // Denylist must override allowlist when both match.
    set_security_policies_for_tests(Some("libc.so.6:getpid"), Some("libc.so.6:getpid"));
    let mut engine = beskid_engine::Engine::new();
    let err = engine
        .compile_artifact(&lowered.artifact)
        .expect_err("should be denied when allow and deny both match");
    let msg = format!("{:?}", err);
    assert!(msg.contains("denied by denylist"));

    // Wildcard policies should match as documented.
    set_security_policies_for_tests(Some("libc.so.*:getpid"), None);
    let mut engine = beskid_engine::Engine::new();
    engine
        .compile_artifact(&lowered.artifact)
        .expect("compile allowed by wildcard allowlist");

    set_security_policies_for_tests(Some("*:getpid"), Some("libc.so.*:getpid"));
    let mut engine = beskid_engine::Engine::new();
    let err = engine
        .compile_artifact(&lowered.artifact)
        .expect_err("wildcard denylist should block wildcard allowlist match");
    let msg = format!("{:?}", err);
    assert!(msg.contains("denied by denylist"));

    Ok(())
}
