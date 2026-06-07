use std::path::{Path, PathBuf};
use std::process::Command;

use crate::support::runtime::{
    aot_compile_only, aot_run_main_i32, aot_run_main_i64, compile_artifact,
};
use crate::test_harness::temp_case_dir;
use beskid_aot::{AotBuildRequest, BuildOutputKind, build};

fn assert_try_parity_ok_case(name: &str, source: &str, expected: i64) {
    let aot_value = aot_run_main_i64(source);
    assert_eq!(
        aot_value, expected,
        "expected AOT try-expression outcome for {name}"
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
        source: "enum Result { Ok(i64 value), Error(string error) } i64 Main() { Result r = Result::Ok(1); i64 value = r?; return value; }",
        expected: 1,
    },
    TryParityOkCase {
        name: "try_expression_nested",
        source: "
            enum Result { Ok(i64 value), Error(string error) }
            i64 unwrap_ok() {
                Result first = Result::Ok(1);
                return first?;
            }
            i64 Main() {
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
            enum Result { Ok(i64 value), Error(string error) }
            i64 Main() {
                Result source = Result::Ok(7);
                mut i64 value = 0;
                if true {
                    value = source?;
                }
                return value;
            }
        ",
        expected: 7,
    },
];

fn build_aot_object(source: &str, output: PathBuf) -> PathBuf {
    let artifact = compile_artifact(source);
    let result = build(AotBuildRequest::with_defaults(
        artifact,
        BuildOutputKind::ObjectOnly,
        output,
        "Main",
    ))
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
    let source = "i64 Main() { return __array_len(__array_new(8, 3)); }";
    let aot_value = aot_run_main_i64(source);
    assert_eq!(aot_value, 3, "expected AOT dispatch array_len result");

    let dir = temp_case_dir("interop_usize");
    let object_path = build_aot_object(source, dir.join("parity.o"));
    assert!(
        object_contains_symbol(&object_path, "interop_dispatch_usize"),
        "expected AOT object to reference interop_dispatch_usize kernel symbol"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn parity_array_len_reads_beskid_array_length() {
    let source = "
        i64 Main() {
            i64 h = __array_new(8, 3);
            return __array_len(h);
        }
    ";
    let aot_value = aot_run_main_i64(source);
    assert_eq!(
        aot_value, 3,
        "expected __array_len to match allocation length"
    );

    let dir = temp_case_dir("array_len");
    let object_path = build_aot_object(source, dir.join("parity_array_len.o"));
    assert!(
        object_contains_symbol(&object_path, "interop_dispatch_usize"),
        "expected AOT object to reference interop_dispatch_usize runtime symbol"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn parity_alloc_path_is_consistent() {
    let source = "i64 Main() { return __array_new(8, 3); }";
    let aot_value = aot_run_main_i64(source);
    assert_ne!(
        aot_value, 0,
        "expected AOT alloc path to produce non-null pointer value"
    );

    let dir = temp_case_dir("array_new");
    let object_path = build_aot_object(source, dir.join("parity.o"));
    assert!(
        object_contains_symbol(&object_path, "interop_dispatch_ptr"),
        "expected AOT object to reference interop_dispatch_ptr runtime symbol"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn parity_panic_builtin_compiles() {
    let source = "unit Main() { if false { __panic_str(\"boom\"); } }";
    aot_compile_only(source);

    let dir = temp_case_dir("panic_builtin");
    let result = build(AotBuildRequest::with_defaults(
        compile_artifact(source),
        BuildOutputKind::ObjectOnly,
        dir.join("panic.o"),
        "Main",
    ))
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
        i64 Main() {
            Worker w = Worker { base: 41 };
            return apply(w);
        }
    ";
    let aot_value = aot_run_main_i64(source);
    assert_eq!(aot_value, 42, "expected AOT contract dispatch outcome");

    let dir = temp_case_dir("contract_dispatch");
    let object_path = build_aot_object(source, dir.join("parity.o"));
    assert!(
        object_path.exists(),
        "expected AOT object output for contract dispatch parity"
    );
    let _ = std::fs::remove_dir_all(dir);
}

struct EventParityCase {
    name: &'static str,
    source: &'static str,
}

const EVENT_PARITY_CASES: &[EventParityCase] = &[
    EventParityCase {
        name: "event_explicit_capacity",
        source: "
            type User { event{4} Created(string payload) }
            impl User { unit Emit(string payload) { this.Created(payload); } }
            i64 Main() {
                mut User u = User { };
                unit(string) handler = (string payload) => { __syscall_write(1, payload); };
                u.Created += handler;
                u.Emit(\"x\");
                u.Created -= handler;
                return 42;
            }
        ",
    },
    EventParityCase {
        name: "event_default_capacity",
        source: "
            type User { event Created(string payload) }
            impl User { unit Emit(string payload) { this.Created(payload); } }
            i64 Main() {
                mut User u = User { };
                unit(string) handler = (string payload) => { __syscall_write(1, payload); };
                u.Created += handler;
                u.Emit(\"x\");
                u.Created -= handler;
                return 42;
            }
        ",
    },
];

#[test]
fn parity_event_lifecycle_is_consistent() {
    for case in EVENT_PARITY_CASES {
        let aot_value = aot_run_main_i64(case.source);
        assert_eq!(
            aot_value, 42,
            "expected AOT event lifecycle outcome for {}",
            case.name
        );

        let dir = temp_case_dir(case.name);
        let object_path = build_aot_object(case.source, dir.join("parity.o"));
        assert!(
            object_contains_symbol(&object_path, "interop_dispatch_unit")
                || object_contains_symbol(&object_path, "interop_dispatch_usize"),
            "expected AOT object to reference interop dispatch for event lifecycle for {}",
            case.name
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn parity_identity_equality_behavior_is_consistent() {
    let source = "
        type User { i64 id }
        i64 Main() {
            User a = User { id: 1 };
            User b = a;
            if a === b {
                return 1;
            }
            return 0;
        }
    ";
    let aot_value = aot_run_main_i64(source);
    assert_eq!(
        aot_value, 1,
        "expected AOT identity equality to evaluate true"
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
        "i32 Main() { mut i32 sum = 0; for i in range(0, 4) { sum = sum + i; } return sum; }";
    let aot_value = aot_run_main_i32(source);
    assert_eq!(aot_value, 6, "expected AOT range-loop accumulation result");

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
        i64 Main() {
            CounterIter iter = CounterIter { sentinel: 0 };
            for i in iter {
                continue;
            }
            return 0;
        }
    ";
    let aot_value = aot_run_main_i64(source);
    assert_eq!(aot_value, 0, "expected AOT generic-iterable loop outcome");

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
fn parity_try_expression_err_path_compiles() {
    let source = "
        enum Result { Ok(i64 value), Error(string error) }
        i64 Main() {
            Result failed = Result::Error(\"boom\");
            i64 value = failed?;
            return value;
        }
    ";
    aot_compile_only(source);

    let dir = temp_case_dir("try_expression_err_compile_only");
    let object_path = build_aot_object(source, dir.join("parity.o"));
    assert!(
        object_path.exists(),
        "expected AOT object output for try-expression err-path compile parity"
    );
    let _ = std::fs::remove_dir_all(dir);
}


#[test]
fn probe_repeat_fn() {
    use crate::support::runtime::aot_run_main_i64;
    let source = r#"string Repeat(string unit, i64 count) {
    string acc = "";
    i64 i = 0;
    while i < count {
        acc = "${acc}${unit}";
        i = i + 1;
    }
    return acc;
}
i64 Main() { return __str_len(Repeat("-", 4)); }"#;
    let v = aot_run_main_i64(source);
    eprintln!("repeat_fn = {}", v);
    assert_eq!(v, 4, "repeat_fn");
}

#[test]
fn probe_repeat_fn_i32_count() {
    use crate::support::runtime::aot_run_main_i64;
    let source = r#"string Repeat(string unit, i32 count) {
    string acc = "";
    i32 i = 0;
    while i < count {
        acc = "${acc}${unit}";
        i = i + 1;
    }
    return acc;
}
i64 Main() { return __str_len(Repeat("-", 4)); }"#;
    let v = aot_run_main_i64(source);
    eprintln!("repeat_fn_i32_count = {}", v);
    assert_eq!(v, 4, "repeat_fn_i32_count");
}

#[test]
fn probe_repeat_fn_mut() {
    use crate::support::runtime::aot_run_main_i64;
    let source = r#"string Repeat(string unit, i64 count) {
    mut string acc = "";
    mut i64 i = 0;
    while i < count {
        acc = "${acc}${unit}";
        i = i + 1;
    }
    return acc;
}
i64 Main() { return __str_len(Repeat("-", 4)); }"#;
    let v = aot_run_main_i64(source);
    eprintln!("repeat_fn_mut = {}", v);
    assert_eq!(v, 4, "repeat_fn_mut");
}

#[test]
fn probe_repeat_cross_mod() {
    use crate::support::runtime::aot_run_main_i64;
    let source = r#"mod lib {
    pub string Repeat(string unit, i64 count) {
        string acc = "";
        i64 i = 0;
        while i < count {
            acc = "${acc}${unit}";
            i = i + 1;
        }
        return acc;
    }
}
i64 Main() { return __str_len(lib.Repeat("-", 4)); }"#;
    let v = aot_run_main_i64(source);
    eprintln!("cross_mod = {}", v);
    assert_eq!(v, 4);
}
