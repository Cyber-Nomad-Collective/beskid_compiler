use std::path::Path;

use beskid_analysis::services::synthetic_compile_plan_for_source;
use beskid_codegen::materialize_source_path_for_lowering;

#[test]
fn materialized_sources_have_isolated_synthetic_scan_roots() {
    let process = std::process::id();
    let first = materialize_source_path_for_lowering(
        Path::new(&format!("synthetic-isolation-first-{process}.bd")),
        "i32 First() { return 1; }",
    )
    .expect("materialize first source");
    let second = materialize_source_path_for_lowering(
        Path::new(&format!("synthetic-isolation-second-{process}.bd")),
        "i32 Second() { return 2; }",
    )
    .expect("materialize second source");

    let first_plan = synthetic_compile_plan_for_source(&first);
    let second_plan = synthetic_compile_plan_for_source(&second);
    let roots_are_isolated = first_plan.source_root != second_plan.source_root;

    std::fs::remove_file(&first).expect("remove first source");
    std::fs::remove_file(&second).expect("remove second source");
    if roots_are_isolated {
        std::fs::remove_dir(first.parent().expect("first source parent")).expect("remove first scan root");
        std::fs::remove_dir(second.parent().expect("second source parent")).expect("remove second scan root");
    }

    assert!(roots_are_isolated, "each materialized source must own its synthetic workspace-scan root");
}

#[test]
fn repl_sentinel_uses_a_portable_materialized_file_name() {
    let materialized = materialize_source_path_for_lowering(Path::new("<repl>"), "i64 Main() { return 42; }")
        .expect("materialize REPL source");

    assert_eq!(materialized.file_name().and_then(|name| name.to_str()), Some("main.bd"));
    assert_eq!(
        std::fs::read_to_string(&materialized).expect("read materialized REPL source"),
        "i64 Main() { return 42; }"
    );

    std::fs::remove_file(&materialized).expect("remove materialized REPL source");
    std::fs::remove_dir(materialized.parent().expect("REPL source parent")).expect("remove REPL scan root");
}
