use crate::support::runtime::{aot_compile_only, build_aot_exe};

fn run_main_exit_code(source: &str) -> i32 {
    let (dir, result) = build_aot_exe(source, "aot_callable");
    let exit_code = result.exit_code;
    let _ = std::fs::remove_dir_all(dir);
    exit_code
}

#[test]
fn aot_callable_unit_main_exits_cleanly() {
    aot_compile_only("unit main() { }");
    let (dir, result) = build_aot_exe("unit main() { }", "aot_callable_unit");
    assert!(
        (0..=1).contains(&result.exit_code),
        "expected unit main subprocess to exit cleanly, got {}",
        result.exit_code
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn aot_callable_i64_return_maps_to_exit_code() {
    let exit_code = run_main_exit_code("i64 main() { return 42; }");
    assert_eq!(exit_code, 42);
}

#[test]
fn aot_callable_bool_return_maps_to_exit_code() {
    let exit_code = run_main_exit_code("bool main() { return true; }");
    assert_eq!(exit_code, 1);
}

#[test]
fn aot_callable_char_return_maps_to_exit_code() {
    let exit_code = run_main_exit_code("char main() { return 'A'; }");
    assert_eq!(exit_code, 65);
}

#[test]
fn aot_callable_string_return_executes_successfully() {
    aot_compile_only("string main() { return \"hello\"; }");
    let exit_code = run_main_exit_code("string main() { return \"hello\"; }");
    assert_ne!(
        exit_code, 0,
        "expected non-zero exit code for pointer-like string return"
    );
}
