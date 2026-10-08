//! CYB-169: Result returned across a call boundary must not SIGILL after Finalize.
//!
//! Minimal shape mirroring Core.Output.Write → Syscall.Write → match Result:
//! callee returns an enum constructed on the stack; caller matches on the pointer.

use std::path::Path;

use beskid_engine::services::run_entrypoint;
use beskid_tools::toolchain::runtime_kit::{RuntimeKitProfile, build_native_host};

use crate::runtime_prefix::RuntimePrefixContext;
use crate::shared_kit;

#[test]
fn returned_enum_survives_match_across_call_boundary() {
    let profile = match std::env::var("BESKID_RUNTIME_KIT_PROFILE") {
        Ok(value) => match beskid_abi::runtime_kit::BuildProfile::parse(&value).expect("valid runtime-kit profile") {
            beskid_abi::runtime_kit::BuildProfile::Debug => RuntimeKitProfile::Debug,
            beskid_abi::runtime_kit::BuildProfile::Release => RuntimeKitProfile::Release,
        },
        Err(std::env::VarError::NotPresent) => RuntimeKitProfile::Debug,
        Err(error) => panic!("invalid BESKID_RUNTIME_KIT_PROFILE: {error}"),
    };
    // The debug kit is the one this binary already shares; a release run publishes its own.
    let kit_lease;
    let release_prefix;
    let prefix = match profile {
        RuntimeKitProfile::Debug => {
            kit_lease = shared_kit::debug();
            kit_lease.prefix()
        }
        RuntimeKitProfile::Release => {
            release_prefix = tempfile::tempdir().expect("exact kit prefix");
            build_native_host(release_prefix.path().to_path_buf(), profile).expect("publish exact native kit");
            release_prefix.path()
        }
    };
    let _runtime_prefix = RuntimePrefixContext::install(prefix);

    let source = r#"
enum Result { Ok(i64 value), Error(i64 error) }
Result MakeOk() { return Result::Ok(7_i64); }
i64 Main() {
    Result result = MakeOk();
    return match result {
        Result::Ok(value) => value,
        Result::Error(_) => -1_i64,
    };
}
"#;
    let output = run_entrypoint(Path::new("cyb169-enum-return.bd"), source, "Main")
        .expect("enum return across call boundary must execute without SIGILL");
    assert_eq!(output, "7", "expected Ok payload 7, got {output}");
}
