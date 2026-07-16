use std::io::Write;
use std::process::{Command, Stdio};

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_beskid_runtime_provenance"))
}

#[test]
fn cli_emits_and_verifies_a_portable_symbol_list_from_standard_input() {
    let fixture = binary()
        .args(["--fixture", "x86_64-unknown-linux-gnu"])
        .output()
        .expect("run fixture command");
    assert!(fixture.status.success());

    let mut verify = binary()
        .args(["--verify", "-"])
        .stdin(Stdio::piped())
        .spawn()
        .expect("run verifier");
    verify
        .stdin
        .take()
        .expect("stdin")
        .write_all(&fixture.stdout)
        .expect("write fixture");
    assert!(verify.wait().expect("wait").success());
}

#[test]
fn cli_rejects_forbidden_bridge_and_unwind_symbols_from_standard_input() {
    let fixture = binary()
        .args(["--fixture", "x86_64-unknown-linux-gnu"])
        .output()
        .expect("run fixture command");
    assert!(fixture.status.success());
    let mut input = String::from_utf8(fixture.stdout).expect("fixture UTF-8");
    input.push_str("defined=beskid_runtime_bridge_init\nundefined=_Unwind_Resume\n");

    let mut verify = binary()
        .args(["--verify", "-"])
        .stdin(Stdio::piped())
        .spawn()
        .expect("run verifier");
    verify
        .stdin
        .take()
        .expect("stdin")
        .write_all(input.as_bytes())
        .expect("write fixture");
    assert!(!verify.wait().expect("wait").success());
}

#[test]
fn cli_verifies_linux_shared_loader_imports_only_in_shared_mode() {
    let fixture = binary()
        .args(["--fixture", "x86_64-unknown-linux-gnu"])
        .output()
        .expect("run fixture command");
    assert!(fixture.status.success());
    let mut input = String::from_utf8(fixture.stdout).expect("fixture UTF-8");
    input.push_str(
        "undefined=_ITM_deregisterTMCloneTable\n\
         undefined=_ITM_registerTMCloneTable\n\
         undefined=__cxa_finalize\n\
         undefined=__gmon_start__\n",
    );

    let mut shared_verify = binary()
        .args(["--verify-shared", "-"])
        .stdin(Stdio::piped())
        .spawn()
        .expect("run shared verifier");
    shared_verify
        .stdin
        .take()
        .expect("stdin")
        .write_all(input.as_bytes())
        .expect("write fixture");
    assert!(shared_verify.wait().expect("wait").success());

    let mut static_verify = binary()
        .args(["--verify", "-"])
        .stdin(Stdio::piped())
        .spawn()
        .expect("run static verifier");
    static_verify
        .stdin
        .take()
        .expect("stdin")
        .write_all(input.as_bytes())
        .expect("write fixture");
    assert!(!static_verify.wait().expect("wait").success());
}
