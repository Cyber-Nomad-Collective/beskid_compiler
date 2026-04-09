use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::codegen::util::lower_resolve_type;
use beskid_aot::{
    AotBuildRequest, BuildOutputKind, BuildProfile, ExportPolicy, LinkMode, RuntimeStrategy, build,
};
use beskid_codegen::lowering::lower_program;
use beskid_engine::Engine;

fn temp_case_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time ok")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "beskid_parity_tests_{name}_{}_{}",
        std::process::id(),
        nanos
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn compile_artifact(source: &str) -> beskid_codegen::CodegenArtifact {
    let (hir, resolution, typed) = lower_resolve_type(source);
    lower_program(&hir, &resolution, &typed).expect("expected codegen lowering to succeed")
}

fn jit_run_main_i64(source: &str) -> i64 {
    let artifact = compile_artifact(source);
    let mut engine = Engine::new();
    engine
        .compile_artifact(&artifact)
        .expect("expected JIT compile to succeed");

    let ptr = unsafe { engine.entrypoint_ptr("main") }.expect("expected main entrypoint pointer");
    assert!(!ptr.is_null(), "expected non-null entrypoint pointer");
    let main_fn: extern "C" fn() -> i64 = unsafe { std::mem::transmute(ptr) };
    engine.with_arena(|_, _| main_fn())
}

fn jit_compile_only(source: &str) {
    let artifact = compile_artifact(source);
    let mut engine = Engine::new();
    engine
        .compile_artifact(&artifact)
        .expect("expected JIT compile to succeed");
}

fn assert_try_parity_ok_case(name: &str, source: &str, expected: i64) {
    let jit_value = jit_run_main_i64(source);
    assert_eq!(
        jit_value, expected,
        "expected JIT try-expression outcome for {name}"
    );

    let dir = temp_case_dir(name);
    let object_path = build_aot_object(source, dir.join("parity.o"));
    assert!(
        object_path.exists(),
        "expected AOT object output for try-expression parity case {name}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

struct TryParityOkCase {
    name: &'static str,
    source: &'static str,
    expected: i64,
}

const TRY_PARITY_OK_CASES: &[TryParityOkCase] = &[
    TryParityOkCase {
        name: "try_expression",
        source: "enum Result { Ok(i64 value), Error(string message) } i64 main() { Result r = Result::Ok(1); i64 value = r?; return value; }",
        expected: 1,
    },
    TryParityOkCase {
        name: "try_expression_nested",
        source: "
            enum Result { Ok(i64 value), Error(string message) }
            i64 unwrap_ok() {
                Result first = Result::Ok(1);
                return first?;
            }
            i64 main() {
                i64 value = unwrap_ok();
                Result second = Result::Ok(value);
                return second?;
            }
        ",
        expected: 1,
    },
    TryParityOkCase {
        name: "try_expression_assignment_branch",
        source: "
            enum Result { Ok(i64 value), Error(string message) }
            i64 main() {
                Result source = Result::Ok(7);
                i64 mut value = 0;
                if true {
                    value = source?;
                }
                return value;
            }
        ",
        expected: 7,
    },
];

fn jit_run_main_i32(source: &str) -> i32 {
    let artifact = compile_artifact(source);
    let mut engine = Engine::new();
    engine
        .compile_artifact(&artifact)
        .expect("expected JIT compile to succeed");

    let ptr = unsafe { engine.entrypoint_ptr("main") }.expect("expected main entrypoint pointer");
    assert!(!ptr.is_null(), "expected non-null entrypoint pointer");
    let main_fn: extern "C" fn() -> i32 = unsafe { std::mem::transmute(ptr) };
    engine.with_arena(|_, _| main_fn())
}

fn build_aot_object(source: &str, output: PathBuf) -> PathBuf {
    let artifact = compile_artifact(source);
    let result = build(AotBuildRequest {
        artifact,
        output_kind: BuildOutputKind::ObjectOnly,
        output_path: output,
        object_path: None,
        target_triple: None,
        profile: BuildProfile::Debug,
        entrypoint: "main".to_owned(),
        export_policy: ExportPolicy::PublicOnly,
        link_mode: LinkMode::Auto,
        runtime: RuntimeStrategy::BuildOnTheFly,
        verbose_link: false,
    })
    .expect("expected AOT object build to succeed");

    result.object_path
}

fn object_contains_symbol(path: &Path, symbol: &str) -> bool {
    let output = Command::new("nm")
        .arg(path)
        .output()
        .expect("expected nm to inspect object file");
    assert!(output.status.success(), "expected nm to succeed");
    let text = String::from_utf8_lossy(&output.stdout);
    text.contains(symbol)
}

#[test]
fn parity_interop_usize_dispatch_path_is_consistent() {
    let source = "i64 main() { return __str_len(\"hello\"); }";
    let jit_value = jit_run_main_i64(source);
    assert_eq!(jit_value, 5, "expected JIT direct str_len result");

    let dir = temp_case_dir("interop_usize");
    let object_path = build_aot_object(source, dir.join("parity.o"));
    assert!(
        object_contains_symbol(&object_path, "str_len"),
        "expected AOT object to reference direct str_len symbol"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn parity_alloc_path_is_consistent() {
    let source = "i64 main() { return __array_new(8, 3); }";
    let jit_value = jit_run_main_i64(source);
    assert_ne!(
        jit_value, 0,
        "expected JIT alloc path to produce non-null pointer value"
    );

    let dir = temp_case_dir("array_new");
    let object_path = build_aot_object(source, dir.join("parity.o"));
    assert!(
        object_contains_symbol(&object_path, "array_new"),
        "expected AOT object to reference array_new runtime symbol"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn parity_panic_builtin_compiles_for_both_backends() {
    let source = "unit main() { if false { __panic_str(\"boom\"); } }";
    let artifact = compile_artifact(source);

    let mut engine = Engine::new();
    engine
        .compile_artifact(&artifact)
        .expect("expected JIT compile to succeed for panic builtin path");

    let dir = temp_case_dir("panic_builtin");
    let result = build(AotBuildRequest {
        artifact,
        output_kind: BuildOutputKind::ObjectOnly,
        output_path: dir.join("panic.o"),
        object_path: None,
        target_triple: None,
        profile: BuildProfile::Debug,
        entrypoint: "main".to_owned(),
        export_policy: ExportPolicy::PublicOnly,
        link_mode: LinkMode::Auto,
        runtime: RuntimeStrategy::BuildOnTheFly,
        verbose_link: false,
    })
    .expect("expected AOT compile to succeed for panic builtin path");

    assert!(
        result.object_path.exists(),
        "expected parity AOT object output"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn parity_contract_dispatch_outcome_is_consistent() {
    let source = "
        contract Service { i64 run(i64 x); }
        type Worker : Service { i64 base }
        impl Worker { i64 run(i64 x) { return this.base + x; } }
        i64 apply(Service s) { return s.run(1); }
        i64 main() {
            Worker w = Worker { base: 41 };
            return apply(w);
        }
    ";
    let jit_value = jit_run_main_i64(source);
    assert_eq!(jit_value, 42, "expected JIT contract dispatch outcome");

    let dir = temp_case_dir("contract_dispatch");
    let object_path = build_aot_object(source, dir.join("parity.o"));
    assert!(
        object_path.exists(),
        "expected AOT object output for contract dispatch parity"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn parity_event_lifecycle_is_consistent_for_explicit_capacity_form() {
    let source = "
        type User { event{4} Created(string payload) }
        impl User {
            unit Emit(string payload) { this.Created(payload); }
        }
        i64 main() {
            User mut u = User { };
            unit(string) handler = (string payload) => { __sys_println(payload); };
            u.Created += handler;
            u.Emit(\"x\");
            u.Created -= handler;
            return 42;
        }
    ";
    let jit_value = jit_run_main_i64(source);
    assert_eq!(
        jit_value, 42,
        "expected JIT explicit-capacity event lifecycle outcome"
    );

    let dir = temp_case_dir("event_explicit_capacity");
    let object_path = build_aot_object(source, dir.join("parity.o"));
    assert!(
        object_contains_symbol(&object_path, "event_subscribe")
            && object_contains_symbol(&object_path, "event_unsubscribe_first"),
        "expected AOT object to reference event lifecycle helpers"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn parity_event_lifecycle_is_consistent_for_default_capacity_form() {
    let source = "
        type User { event Created(string payload) }
        impl User {
            unit Emit(string payload) { this.Created(payload); }
        }
        i64 main() {
            User mut u = User { };
            unit(string) handler = (string payload) => { __sys_println(payload); };
            u.Created += handler;
            u.Emit(\"x\");
            u.Created -= handler;
            return 42;
        }
    ";
    let jit_value = jit_run_main_i64(source);
    assert_eq!(
        jit_value, 42,
        "expected JIT default-capacity event lifecycle outcome"
    );

    let dir = temp_case_dir("event_default_capacity");
    let object_path = build_aot_object(source, dir.join("parity.o"));
    assert!(
        object_contains_symbol(&object_path, "event_subscribe")
            && object_contains_symbol(&object_path, "event_unsubscribe_first"),
        "expected AOT object to reference event lifecycle helpers"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn parity_identity_equality_behavior_is_consistent() {
    let source = "
        type User { i64 id }
        i64 main() {
            User a = User { id: 1 };
            User b = a;
            if a === b {
                return 1;
            }
            return 0;
        }
    ";
    let jit_value = jit_run_main_i64(source);
    assert_eq!(
        jit_value, 1,
        "expected JIT identity equality to evaluate true"
    );

    let dir = temp_case_dir("identity_equality");
    let object_path = build_aot_object(source, dir.join("parity.o"));
    assert!(
        object_path.exists(),
        "expected AOT object output for identity equality parity"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn parity_range_loop_behavior_is_consistent() {
    let source =
        "i32 main() { i32 mut sum = 0; for i in range(0, 4) { sum = sum + i; } return sum; }";
    let jit_value = jit_run_main_i32(source);
    assert_eq!(jit_value, 6, "expected JIT range-loop accumulation result");

    let dir = temp_case_dir("range_loop");
    let object_path = build_aot_object(source, dir.join("parity.o"));
    assert!(
        object_path.exists(),
        "expected AOT object output for range-loop parity"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn parity_generic_iterable_loop_behavior_is_consistent() {
    let source = "
        enum Option { Some(i64 value), None }
        type CounterIter { i64 sentinel }
        impl CounterIter {
            Option Next() {
                return Option::None();
            }
        }
        i64 main() {
            CounterIter iter = CounterIter { sentinel: 0 };
            for i in iter {
                continue;
            }
            return 0;
        }
    ";
    let jit_value = jit_run_main_i64(source);
    assert_eq!(jit_value, 0, "expected JIT generic-iterable loop outcome");

    let dir = temp_case_dir("generic_iterable_loop");
    let object_path = build_aot_object(source, dir.join("parity.o"));
    assert!(
        object_path.exists(),
        "expected AOT object output for generic iterable loop parity"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn parity_try_success_cases_are_consistent() {
    for case in TRY_PARITY_OK_CASES {
        assert_try_parity_ok_case(case.name, case.source, case.expected);
    }
}

#[test]
fn parity_try_expression_err_path_compiles_for_both_backends() {
    let source = "
        enum Result { Ok(i64 value), Error(string message) }
        i64 main() {
            Result failed = Result::Error(\"boom\");
            i64 value = failed?;
            return value;
        }
    ";
    jit_compile_only(source);

    let dir = temp_case_dir("try_expression_err_compile_only");
    let object_path = build_aot_object(source, dir.join("parity.o"));
    assert!(
        object_path.exists(),
        "expected AOT object output for try-expression err-path compile parity"
    );
    let _ = std::fs::remove_dir_all(dir);
}
