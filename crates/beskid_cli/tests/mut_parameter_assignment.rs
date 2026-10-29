//! `mut` parameters pass by value (E1259). The preview.8 repro, a whole assignment to a `mut`
//! struct parameter that the caller never sees, is a user error in `beskid check` and `beskid
//! run`; the shapes that do reach the caller run and give the caller the value 80.
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

const SIZE: &str = "type Size\n{\n    i32 columns;\n    i32 rows;\n}\n\n";

#[test]
fn a_whole_assignment_the_caller_cannot_see_is_e1259() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("Main.bd");
    let cases = [
        ("unit AssignWhole(mut Size s)\n{\n    s = Size { columns: 80, rows: 24, };\n}\n", "s"),
        ("enum Color\n{\n    Red(),\n    Green(),\n}\n\nunit Paint(mut Color c)\n{\n    c = Color::Green();\n}\n", "c"),
        ("unit Bump(mut i64 value)\n{\n    value = value + 1;\n}\n", "value"),
    ];
    for (body, parameter) in cases {
        std::fs::write(&path, format!("{SIZE}{body}\npub i32 Main()\n{{\n    return 0;\n}}\n")).unwrap();
        let output = beskid(root.path()).args(["check", "--plain"]).arg(&path).output().unwrap();
        let report = text(&output);
        assert_eq!(output.status.code(), Some(1), "check must fail: {report}");
        assert_eq!(report.matches("E1259").count(), 1, "E1259 once: {report}");
        assert!(
            report.contains(&format!("the value assigned to the `mut` parameter `{parameter}` is never read")),
            "{report}"
        );
        assert!(report.contains(&format!("`{parameter}` is passed by value")), "the note: {report}");
        assert!(report.contains("parameters pass by value"), "the help: {report}");
        assert!(!report.contains("internal compiler error"), "{report}");
    }
}

#[test]
fn the_preview8_repro_fails_in_run_instead_of_keeping_the_old_value() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("Main.bd");
    std::fs::write(
        &path,
        format!(
            "{SIZE}unit AssignWhole(mut Size s)\n{{\n    s = Size {{ columns: 80, rows: 24, }};\n}}\n\n\
             i32 Main()\n{{\n    mut Size s = Size {{ columns: 0, rows: 0, }};\n    AssignWhole(s);\n    return s.columns;\n}}\n"
        ),
    )
    .unwrap();
    let output = beskid(root.path()).args(["run", "--plain"]).arg(&path).output().unwrap();
    let report = text(&output);
    assert_eq!(output.status.code(), Some(1), "run must stop at the gate: {report}");
    assert_eq!(report.matches("the value assigned to the `mut` parameter `s` is never read").count(), 1, "{report}");
}

#[test]
fn field_writes_and_returned_values_give_the_caller_80() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("Main.bd");
    let cases = [
        (
            "unit AssignFields(mut Size s)\n{\n    s.columns = 80;\n    s.rows = 24;\n}\n",
            "mut Size s = Size { columns: 0, rows: 0, };\n    AssignFields(s);\n    return s.columns;",
            "struct field writes",
        ),
        (
            "unit AssignFields(mut Size s)\n{\n    s.columns = 80;\n}\n\nunit PassOn(mut Size s)\n{\n    AssignFields(s);\n}\n",
            "mut Size s = Size { columns: 0, rows: 0, };\n    PassOn(s);\n    return s.columns;",
            "a nested call that passes the parameter on",
        ),
        (
            "Size Replaced(mut Size s)\n{\n    s = Size { columns: 80, rows: 24, };\n    return s;\n}\n",
            "mut Size s = Size { columns: 0, rows: 0, };\n    s = Replaced(s);\n    return s.columns;",
            "a whole assignment returned to the caller",
        ),
        (
            "i32 Widen(mut i32 columns)\n{\n    if (columns < 80)\n    {\n        columns = 80;\n    }\n    return columns;\n}\n",
            "mut i32 columns = 0;\n    columns = Widen(columns);\n    return columns;",
            "a primitive",
        ),
        (
            "enum Width\n{\n    Narrow(),\n    Wide(),\n}\n\nWidth Widen(mut Width w)\n{\n    w = Width::Wide();\n    return w;\n}\n",
            "mut Width w = Width::Narrow();\n    w = Widen(w);\n    match w\n    {\n        Width::Wide() => { return 80; },\n        Width::Narrow() => { return 0; },\n    };\n    return 1;",
            "an enum",
        ),
        (
            "type Columns\n{\n    i32 value;\n\n    pub unit Widen(mut this)\n    {\n        this.value = 80;\n        return;\n    }\n}\n",
            "mut Columns c = Columns { value: 0, };\n    c.Widen();\n    return c.value;",
            "a `mut this` receiver",
        ),
    ];
    for (declarations, main, case) in cases {
        std::fs::write(&path, format!("{SIZE}{declarations}\ni32 Main()\n{{\n    {main}\n}}\n")).unwrap();
        let output = beskid(root.path()).args(["run", "--plain"]).arg(&path).output().unwrap();
        assert_eq!(output.status.code(), Some(80), "{case}: {}", text(&output));
    }
}
