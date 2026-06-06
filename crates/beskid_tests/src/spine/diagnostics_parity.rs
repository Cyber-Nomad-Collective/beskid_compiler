//! Gate diagnostics must match `prepare_compilation` for project-backed sources.

use std::fs;

use beskid_analysis::services::{
    PrepareMode, PrepareOptions, analyze_source_with_compilation_context, prepare_compilation_diagnostics,
    resolved_input_from_plan, FrontEndOptions,
};
use beskid_analysis::CompilationContext;

use crate::projects::with_cwd;
use crate::test_harness::{temp_case_dir, write_project_manifest as write_manifest};

#[test]
fn analyze_matches_prepare_for_temp_project() {
    let root = temp_case_dir("spine_diagnostics_parity");
    let src_dir = root.join("Src");
    fs::create_dir_all(&src_dir).expect("source root");
    write_manifest(
        &root,
        r#"
project {
  name = "SpineParity"
  version = "0.1.0"
}

target "app" {
  kind = App
  entry = "Main.bd"
}
"#,
    );

    let source = r#"
i32 helper() {
    return 1;
}

i32 main() {
    return helper();
}
"#;
    let entry = src_dir.join("Main.bd");
    fs::write(&entry, source).expect("write source");

    with_cwd(&root, || {
        let mut ctx = CompilationContext::try_for_analysis_path(&entry, None).expect("context");
        let plan = ctx.compile_plan.clone().expect("plan");
        let resolved = resolved_input_from_plan(
            entry.clone(),
            source.to_string(),
            plan,
            ctx.prepared_workspace.clone(),
            None,
        );

        let gate =
            analyze_source_with_compilation_context(&entry, source, &mut ctx).expect("analyze");
        let (_, prepare) = prepare_compilation_diagnostics(
            &resolved,
            PrepareOptions {
                mode: PrepareMode::DiagnosticsOnly,
                front_end: FrontEndOptions {
                    with_semantic_diagnostics: true,
                    ..Default::default()
                },
            },
            None,
        )
        .expect("prepare");

        assert_eq!(
            diagnostic_codes(&gate),
            diagnostic_codes(&prepare),
            "analyze gate and prepare spine must emit the same diagnostic codes"
        );
    });

    let _ = fs::remove_dir_all(root);
}

fn diagnostic_codes(diagnostics: &[beskid_analysis::SemanticDiagnostic]) -> Vec<String> {
    let mut codes: Vec<String> = diagnostics
        .iter()
        .filter_map(|diag| diag.code.clone())
        .collect();
    codes.sort();
    codes
}
