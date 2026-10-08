//! Logical module paths are package-native: Beskid has no `Std` namespace.
//!
//! The host project's modules are unprefixed, and every Corelib package keeps its own module roots
//! (`Core.*`, `Testing.*`, `Concurrency.*`, `Beskid.Compiler.*`). The Corelib dependency label is
//! `Core`; dependency labels never become module path segments.
use std::path::{Path, PathBuf};
use beskid_analysis::projects::{build_compile_plan, effective_roots_from_plan_and_workspace,
    infer_logical_module_path, resolve_module_file, SourceUnit};
use beskid_analysis::services::parse_program_with_source_name;

fn source_unit(path: PathBuf) -> SourceUnit {
    let source = "i32 Value() { return 1; }".to_owned();
    let program = parse_program_with_source_name(path.to_str().unwrap(), &source).unwrap();
    SourceUnit::bind_request(path.clone(), path.display().to_string(), source, program)
}

fn corelib_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corelib").canonicalize().unwrap()
}

#[test]
fn core_graph_keeps_host_unprefixed_and_corelib_package_native() {
    let directory = tempfile::tempdir().unwrap();
    let host = directory.path();
    std::fs::create_dir_all(host.join("Src")).unwrap();
    std::fs::write(host.join("Src/Main.bd"), "i32 Main() { return 0; }").unwrap();
    std::fs::write(host.join("Src/Helpers.bd"), "i32 Value() { return 1; }").unwrap();
    let corelib = corelib_root();
    let manifest = host.join("App.bproj");
    std::fs::write(&manifest, format!(r#"App {{ name = "App" version = "0.6.0" root = "Src" }}
 dependency "Core" {{ source = path path = "{}" }}
 target "main" {{ kind = App entry = "Main.bd" }}
"#, corelib.join("beskid_corelib").display())).unwrap();
    let plan = build_compile_plan(&manifest, None).expect("real Core graph");
    assert!(plan.has_core_dependency, "the `Core` dependency label marks the Corelib aggregate");
    let roots = effective_roots_from_plan_and_workspace(&plan, None);
    let cases = [
        (host.join("Src/Helpers.bd"), vec!["Helpers"]),
        (corelib.join("packages/foundation/src/Core/Output/Output.bd"), vec!["Core", "Output"]),
        (corelib.join("packages/foundation/src/Testing/Assert.bd"), vec!["Testing", "Assert"]),
        (corelib.join("packages/concurrency/src/Concurrency/Fiber.bd"), vec!["Concurrency", "Fiber"]),
        (corelib.join("packages/compiler-sdk/src/Beskid/Compiler/Collect.bd"),
            vec!["Beskid", "Compiler", "Collect"]),
    ];
    let mut failures = Vec::new();
    for (path, expected) in cases {
        assert!(path.is_file(), "canonical fixture must exist: {}", path.display());
        let actual = infer_logical_module_path(&source_unit(path), &roots);
        let expected = Some(expected.into_iter().map(str::to_owned).collect::<Vec<_>>());
        if actual != expected { failures.push(format!("namespace: expected {expected:?}, got {actual:?}")); }
    }
    match resolve_module_file("Core.Output", &roots) {
        Some(path) if path.starts_with(corelib.join("packages/foundation")) => {}
        other => failures.push(format!("Core.Output must resolve to the foundation package, got {other:?}")),
    }
    for retired in ["Std.Core.Output", "Std.Testing.Assert", "Std::Core::Output"] {
        if let Some(path) = resolve_module_file(retired, &roots) {
            failures.push(format!("`{retired}` must not resolve: {}", path.display()));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn std_dependency_label_without_path_names_the_core_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let host = directory.path();
    std::fs::create_dir_all(host.join("Src")).unwrap();
    std::fs::write(host.join("Src/Main.bd"), "i32 Main() { return 0; }").unwrap();
    let manifest = host.join("App.bproj");
    std::fs::write(&manifest, r#"App { name = "App" version = "0.6.0" root = "Src" }
 dependency "Std" { source = path }
 target "main" { kind = App entry = "Main.bd" }
"#).unwrap();
    let error = build_compile_plan(&manifest, None).expect_err("`Std` is not the Corelib dependency label");
    let message = error.to_string();
    assert!(message.contains("`Std`") && message.contains("`Core`"), "error must name the `Core` label: {message}");
}
