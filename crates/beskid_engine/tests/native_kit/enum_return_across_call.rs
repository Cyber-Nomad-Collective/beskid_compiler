//! An enum value returned across a call boundary survives the caller's `match`.
//!
//! Minimal shape of `Core.Output.Write` -> `Syscall.Write` -> `match Result`: the callee returns
//! an enum constructed on its stack and the caller matches on it. It once trapped with SIGILL
//! after `Finalize`.

use std::path::Path;

use crate::explicit_kit::run_entrypoint_with_kit;
use crate::shared_kit;

#[test]
fn returned_enum_survives_match_across_call_boundary() {
    let kit = shared_kit::debug();
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
    let output = run_entrypoint_with_kit(
        kit.prefix(),
        Path::new("enum-return-across-call.bd"),
        source,
        "Main",
        &beskid_engine::ModHost::NoMods,
    )
    .expect("an enum returned across a call boundary executes");
    assert_eq!(output, "7", "expected Ok payload 7, got {output}");
}
