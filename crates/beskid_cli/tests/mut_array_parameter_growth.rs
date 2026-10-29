//! Growth of a `mut T[]` parameter never changes the caller's array (memory model "Dead growth
//! of a mutable array parameter", E1231). On preview.8 the callee's growth leaked into the caller
//! up to the caller's old capacity, because both handles named one array header. Every case
//! encodes the caller's and the callee's lengths in the exit code; none may depend on capacity.
//! Runs against the provisioned Corelib and runtime kit of the test environment.
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn beskid(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_beskid_cli"));
    command
        .env("BESKID_CONFIG_DIR", root.join("config"))
        .env("BESKID_HOME", root.join("toolchain-home"))
        .env("OTEL_SDK_DISABLED", "true")
        .env_remove("RUST_LOG")
        .stdin(Stdio::null());
    command
}

fn text(output: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
}

const CALLEES: &str = "use Core.Collections.Array;\n\n\
i64[] AppendOne(mut i64[] xs)\n{\n    Array.Append<i64>(xs, 9);\n    return xs;\n}\n\n\
i64[] AppendHundred(mut i64[] xs)\n{\n    mut i64 i = 0;\n    while (i < 100)\n    {\n        Array.Append<i64>(xs, i);\n        i = i + 1;\n    }\n    return xs;\n}\n\n\
i64[] TryAppendOne(mut i64[] xs)\n{\n    if (Array.TryAppend<i64>(xs, 9))\n    {\n        return xs;\n    }\n    return xs;\n}\n\n\
unit WriteFirst(mut i64[] xs)\n{\n    xs[0] = 42;\n}\n\n\
i64[] WriteThenGrow(mut i64[] xs)\n{\n    xs[0] = 7;\n    Array.Append<i64>(xs, 9);\n    xs[0] = 8;\n    return xs;\n}\n\n\
i64[] Filled(i64 count)\n{\n    mut i64[] xs = Array.Empty<i64>();\n    mut i64 i = 0;\n    while (i < count)\n    {\n        Array.Append<i64>(xs, i);\n        i = i + 1;\n    }\n    return xs;\n}\n\n";

fn run(root: &Path, main: &str) -> (Option<i32>, String) {
    let path = root.join("Main.bd");
    std::fs::write(&path, format!("{CALLEES}i32 Main()\n{{\n    {main}\n}}\n")).unwrap();
    let output = beskid(root).args(["run", "--plain"]).arg(&path).output().unwrap();
    (output.status.code(), text(&output))
}

#[test]
fn growth_of_a_mut_array_parameter_never_reaches_the_callers_array() {
    let root = tempfile::tempdir().unwrap();
    // Exit code = 100 + 10 * caller length + callee length, unless stated otherwise.
    let cases = [
        // One element, capacity 4: three spare slots. preview.8: caller length 2 (exit 122).
        (
            "i64[] a = Filled(1);\n    i64[] b = AppendOne(a);\n    return i32(100 + 10 * Array.Len<i64>(a) + Array.Len<i64>(b));",
            112,
            "spare capacity",
        ),
        // Four elements, capacity 4: the growth reallocates anyway.
        (
            "i64[] a = Filled(4);\n    i64[] b = AppendOne(a);\n    return i32(100 + 10 * Array.Len<i64>(a) + Array.Len<i64>(b));",
            145,
            "no spare capacity",
        ),
        // An empty array.
        (
            "i64[] a = Array.Empty<i64>();\n    i64[] b = AppendOne(a);\n    return i32(100 + 10 * Array.Len<i64>(a) + Array.Len<i64>(b));",
            101,
            "an empty start",
        ),
        // 100 appends from one element: caller 1, callee 101 (preview.8: caller 4, exit 105).
        (
            "i64[] a = Filled(1);\n    i64[] b = AppendHundred(a);\n    return i32(Array.Len<i64>(a) + Array.Len<i64>(b));",
            102,
            "100 appends",
        ),
        // The checked append detaches in the same way.
        (
            "i64[] a = Filled(1);\n    i64[] b = TryAppendOne(a);\n    return i32(100 + 10 * Array.Len<i64>(a) + Array.Len<i64>(b));",
            112,
            "TryAppend with spare capacity",
        ),
        // A growth published by return and assigned in the caller reaches the caller.
        (
            "mut i64[] a = Filled(1);\n    a = AppendOne(a);\n    a = AppendHundred(a);\n    return i32(Array.Len<i64>(a));",
            102,
            "a published growth",
        ),
    ];
    for (main, expected, case) in cases {
        let (code, report) = run(root.path(), main);
        assert_eq!(code, Some(expected), "{case}: {report}");
    }
}

#[test]
fn element_writes_through_a_mut_array_parameter_still_reach_the_caller() {
    let root = tempfile::tempdir().unwrap();
    let cases = [
        ("i64[] a = Filled(1);\n    WriteFirst(a);\n    return i32(a[0]);", 42, "an element write"),
        // The write before the growth reaches the caller; the write after it changes the
        // callee's copy only.
        (
            "i64[] a = Filled(1);\n    i64[] b = WriteThenGrow(a);\n    return i32(a[0] * 10 + b[0] + 100 * (Array.Len<i64>(a) - 1));",
            78,
            "writes before and after the growth",
        ),
    ];
    for (main, expected, case) in cases {
        let (code, report) = run(root.path(), main);
        assert_eq!(code, Some(expected), "{case}: {report}");
    }
}

#[test]
fn a_growth_that_only_reads_the_parameter_locally_is_e1231() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("Main.bd");
    for body in [
        "i64 AppendMutAndRead(mut i64[] xs)\n{\n    Array.Append<i64>(xs, 9);\n    return Array.Len<i64>(xs);\n}\n",
        "i64 AppendManyMutAndRead(mut i64[] xs)\n{\n    mut i64 i = 0;\n    while (i < 100)\n    {\n        Array.Append<i64>(xs, i);\n        i = i + 1;\n    }\n    return Array.Len<i64>(xs);\n}\n",
    ] {
        std::fs::write(
            &path,
            format!("use Core.Collections.Array;\n\n{body}\npub i32 Main()\n{{\n    return 0;\n}}\n"),
        )
        .unwrap();
        for command in ["check", "run"] {
            let output = beskid(root.path()).args([command, "--plain"]).arg(&path).output().unwrap();
            let report = text(&output);
            assert_eq!(output.status.code(), Some(1), "{command} must fail: {report}");
            assert_eq!(report.matches("E1231").count(), 1, "{command}: E1231 once: {report}");
            assert!(!report.contains("E1259"), "{command}: {report}");
            assert!(!report.contains("internal compiler error"), "{report}");
        }
    }
}
