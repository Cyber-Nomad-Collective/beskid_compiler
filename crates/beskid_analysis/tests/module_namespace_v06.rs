//! Mixed Std/SDK/ordinary project namespaces must agree with physical discovery.
use std::path::{Path, PathBuf};
use beskid_analysis::projects::{build_compile_plan, effective_roots_from_plan_and_workspace,
    infer_logical_module_path, resolve_module_file, SourceUnit};
use beskid_analysis::services::parse_program_with_source_name;

fn source_unit(path: PathBuf) -> SourceUnit {
    let source = "i32 Value() { return 1; }".to_owned();
    let program = parse_program_with_source_name(path.to_str().unwrap(), &source).unwrap();
    SourceUnit::bind_request(path.clone(), path.display().to_string(), source, program)
}

#[test]
fn mixed_std_graph_preserves_host_and_direct_sdk_namespaces() {
    let directory = tempfile::tempdir().unwrap();
    let host = directory.path();
    std::fs::create_dir_all(host.join("Src/Testing")).unwrap();
    std::fs::write(host.join("Src/Main.bd"), "i32 Main() { return 0; }").unwrap();
    std::fs::write(host.join("Src/Helpers.bd"), "i32 Value() { return 1; }").unwrap();
    std::fs::write(host.join("Src/Testing/Assert.bd"), "i32 Fake() { return 0; }").unwrap();
    let corelib = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corelib").canonicalize().unwrap();
    let manifest = host.join("App.bproj");
    std::fs::write(&manifest, format!(r#"App {{ name = "App" version = "0.6.0" root = "Src" }}
 dependency "Std" {{ source = path path = "{}" }}
 target "main" {{ kind = App entry = "Main.bd" }}
"#, corelib.join("beskid_corelib").display())).unwrap();
    let plan = build_compile_plan(&manifest, None).expect("real mixed Std graph");
    assert!(plan.has_std_dependency);
    let roots = effective_roots_from_plan_and_workspace(&plan, None);
    let cases = [
        (host.join("Src/Helpers.bd"), vec!["Helpers"]),
        (corelib.join("packages/compiler-sdk/src/Beskid/Compiler/Collect.bd"),
            vec!["Beskid", "Compiler", "Collect"]),
    ];
    let mut failures = Vec::new();
    for (path, expected) in cases {
        assert!(path.is_file(), "canonical fixture must exist: {}", path.display());
        let actual = infer_logical_module_path(&source_unit(path), &roots, plan.has_std_dependency);
        let expected = Some(expected.into_iter().map(str::to_owned).collect::<Vec<_>>());
        if actual != expected { failures.push(format!("namespace: expected {expected:?}, got {actual:?}")); }
    }
    if let Some(path) = resolve_module_file("Std.Testing.Assert", &roots) {
        if path.starts_with(host) { failures.push(format!("ordinary host impersonates Std module: {}", path.display())); }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
