//! A recoverable parse error in a package's own source (an entry-less `Lib` package, as every
//! Corelib member is) is reported by `check` and stops `build` before lowering, exactly as in an
//! application. The ICE sweep found the preview.8/9 failure: `check` said "no issues" and `build`
//! lowered the parser's repaired tree (E2102, or a silently compiled repair).

use std::process::Output;

use crate::harness::assertions::assert_failure;
use crate::harness::cli::BeskidCliInvoker;
use crate::harness::workspace::E2eWorkspace;

const MANIFEST: &str = "PackageParseRecovery.bproj";

/// One broken package: `file` (under `src/Shapes`) with `clean` replaced by `broken`.
struct Shape {
    file: &'static str,
    clean: &'static str,
    broken: &'static str,
    /// `file:line:column` of the parse error and its message.
    location: &'static str,
    message: &'static str,
}

fn text(output: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
}

fn assert_parse_error(output: &Output, shape: &Shape, command: &str) {
    assert_failure(output, command);
    let text = text(output);
    assert!(text.contains("E0101"), "{command} must report the parse error as E0101:\n{text}");
    assert!(text.contains(shape.message), "{command} must report `{}`:\n{text}", shape.message);
    assert!(text.contains(shape.location), "{command} must report the error at {}:\n{text}", shape.location);
    assert!(!text.contains("no issues"), "{command} must not report a clean package:\n{text}");
    assert!(!text.contains("E2102"), "{command} must stop before lowering (no E2102):\n{text}");
    assert!(!text.contains("internal compiler error"), "{command} must not be an internal error:\n{text}");
}

fn assert_shape_is_a_parse_error(shape: Shape) {
    let workspace = E2eWorkspace::from_fixture("package_parse_recovery");
    let manifest = workspace.join(MANIFEST);
    let manifest = manifest.to_str().expect("manifest path str");
    let path = workspace.join("src/Shapes").join(shape.file);
    let source = std::fs::read_to_string(&path).expect("read fixture source");
    assert_eq!(source.matches(shape.clean).count(), 1, "the fixture holds `{}` once", shape.clean);
    std::fs::write(&path, source.replace(shape.clean, shape.broken)).expect("write broken source");
    let cli = BeskidCliInvoker::new();

    let check = cli.run(["check", "--project", manifest]);
    assert_parse_error(&check, &shape, "check");
    let build = cli.run(["build", "--project", manifest]);
    assert_parse_error(&build, &shape, "build");
}

/// The fixture itself is a clean package: check and build succeed, so each shape below fails only
/// because of its one parse error.
#[test]
fn clean_package_checks_and_builds() {
    let workspace = E2eWorkspace::from_fixture("package_parse_recovery");
    let manifest = workspace.join(MANIFEST);
    let manifest = manifest.to_str().expect("manifest path str");
    let cli = BeskidCliInvoker::new();
    for command in ["check", "build"] {
        let output = cli.run([command, "--project", manifest]);
        assert!(output.status.success(), "{command} of the clean package fails:\n{}", text(&output));
    }
}

/// Sweep mutants 5 and 24 (`Beskid.SyntaxNodes..NodeSpan` in a compiler-sdk field type).
#[test]
fn double_dot_in_a_field_type_path_is_a_parse_error() {
    assert_shape_is_a_parse_error(Shape {
        file: "Spanned.bd",
        clean: "pub Shapes.Span.Span span;",
        broken: "pub Shapes.SpanSpan..Span span;",
        location: "Spanned.bd:3:25",
        message: "expected a type while parsing field `SpanSpan`, found `.`",
    });
}

/// Sweep mutant 25 (codec `Mem.bd`: a comma deleted from an array literal).
#[test]
fn missing_comma_in_an_array_literal_is_a_parse_error() {
    assert_shape_is_a_parse_error(Shape {
        file: "Bytes.bd",
        clean: "[1_u8, 2_u8, 3_u8, 4_u8]",
        broken: "[1_u8, 2_u8 3_u8, 4_u8]",
        location: "Bytes.bd:3:24",
        message: "found `3_u8`",
    });
}

/// Sweep mutant 32 (crypto `Hkdf.bd`: `info[9_i64  i]`, the operator of an index deleted).
#[test]
fn missing_operator_in_an_index_is_a_parse_error() {
    assert_shape_is_a_parse_error(Shape {
        file: "Bytes.bd",
        clean: "info[4_i64 + i]",
        broken: "info[4_i64  i]",
        location: "Bytes.bd:12:21",
        message: "expected `]` or an operator while parsing index, found `i`",
    });
}

/// Sweep mutant 36 (uri `Pct.bd`: an `if (...)` header line duplicated).
#[test]
fn duplicated_if_header_is_a_parse_error() {
    let header = "        if (bytes[i] == 37_u8 && i + 1_i64 < 4_i64)\n";
    assert_shape_is_a_parse_error(Shape {
        file: "Bytes.bd",
        clean: header,
        broken: "        if (bytes[i] == 37_u8 && i + 1_i64 < 4_i64)\n        if (bytes[i] == 37_u8 && i + 1_i64 < 4_i64)\n",
        location: "Bytes.bd:25:9",
        message: "expected `{` or an operator while parsing `if` statement, found `if`",
    });
}
