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
        Ok(Some(path)) if path.starts_with(corelib.join("packages/foundation")) => {}
        other => failures.push(format!("Core.Output must resolve to the foundation package, got {other:?}")),
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}


/// A checkout under a directory named `src` (CI clones to `/woodpecker/src/github.com/...`, users
/// keep projects in `~/src/...`) never leaks the checkout location into module paths, for units
/// under the project root and for units outside every effective root.
#[test]
fn project_below_a_src_directory_keeps_package_native_module_paths() {
    let directory = tempfile::tempdir().unwrap();
    let checkout = directory.path().join("src/github.com/Org/beskid");
    let host = checkout.join("app");
    std::fs::create_dir_all(host.join("src/Nested")).unwrap();
    std::fs::write(host.join("src/Main.bd"), "i32 Main() { return 0; }").unwrap();
    std::fs::write(host.join("src/Nested/Helpers.bd"), "i32 Value() { return 1; }").unwrap();
    let outside = checkout.join("corelib/packages/foundation/src/Core/Results/Result.bd");
    std::fs::create_dir_all(outside.parent().unwrap()).unwrap();
    std::fs::write(&outside, "i32 Value() { return 1; }").unwrap();
    let manifest = host.join("App.bproj");
    std::fs::write(&manifest, r#"App { name = "App" version = "0.6.0" root = "src" }
 target "main" { kind = App entry = "Main.bd" }
"#).unwrap();
    let plan = build_compile_plan(&manifest, None).expect("project under a src directory");
    let roots = effective_roots_from_plan_and_workspace(&plan, None);
    let cases = [
        (host.join("src/Nested/Helpers.bd"), vec!["Nested", "Helpers"]),
        (outside, vec!["Core", "Results", "Result"]),
    ];
    let mut failures = Vec::new();
    for (path, expected) in cases {
        let actual = infer_logical_module_path(&source_unit(path.clone()), &roots);
        let expected = Some(expected.into_iter().map(str::to_owned).collect::<Vec<_>>());
        if actual != expected { failures.push(format!("{}: expected {expected:?}, got {actual:?}", path.display())); }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
