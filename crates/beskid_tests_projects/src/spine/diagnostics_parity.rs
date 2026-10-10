//! Gate diagnostics must match `prepare_compilation` for project-backed sources.
//!
//! The command gate (`beskid analyze` / `beskid check`: the process-scoped query database) and an
//! editor-owned query database (the language server's session) run the one query-backed prepare
//! spine; both must report the same diagnostic codes. Native Mods read syntax through the
//! registered query authority, which only that spine installs.

use std::fs;

use beskid_analysis::CompilationContext;
use beskid_analysis::services::{FrontEndOptions, PrepareOptions, resolved_input_from_plan};
use beskid_queries::BeskidDatabase;

use crate::projects::fixture_harness::enter_mod_host;
use crate::projects::with_cwd;
use beskid_tests_support::{temp_case_dir, write_project_manifest as write_manifest};

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

i32 Main() {
    return helper();
}
"#;
    let entry = src_dir.join("Main.bd");
    fs::write(&entry, source).expect("write source");

    with_cwd(&root, || {
        let ctx = CompilationContext::try_for_analysis_path(&entry, None).expect("context");
        let plan = ctx.compile_plan.clone().expect("plan");
        let mod_host = enter_mod_host();
        let native_mods = mod_host.corelib_mod_host(&plan).expect("qualify the native Mods of the compile plan");
        let resolved = resolved_input_from_plan(entry.clone(), source.to_string(), plan, None, None);
        let options = |native_mods: beskid_analysis::mod_host::ModHost| PrepareOptions {
            front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
            ..PrepareOptions::with_mod_host(native_mods)
        };

        let (_, gate, _fixes) =
            beskid_queries::prepare_compilation_diagnostics(&resolved, options(native_mods.clone()), None)
                .expect("analyze");
        let mut editor_db = BeskidDatabase::default();
        let (_, prepare, _fixes) = beskid_queries::prepare_compilation_diagnostics_with_db(
            &mut editor_db,
            &resolved,
            options(native_mods),
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
    let mut codes: Vec<String> = diagnostics.iter().filter_map(|diag| diag.code.clone()).collect();
    codes.sort();
    codes
}

/// `beskid check` judges every own unit of an App, not only its entry: a type error in a sibling
/// unit nothing calls (the shape of a Mod's `Driver.bd`) is reported in that unit's own source,
/// by the command gate and by the editor database alike. The legacy checker discarded it.
#[test]
fn check_reports_type_error_in_uncalled_sibling_unit_of_an_app() {
    let root = temp_case_dir("spine_diagnostics_sibling_unit");
    let src_dir = root.join("Src");
    fs::create_dir_all(&src_dir).expect("source root");
    write_manifest(
        &root,
        r#"
project {
  name = "SiblingUnit"
  version = "0.1.0"
}

target "app" {
  kind = App
  entry = "Main.bd"
}
"#,
    );
    let source = "i32 Main() {\n    return 0;\n}\n";
    let entry = src_dir.join("Main.bd");
    fs::write(&entry, source).expect("write entry");
    fs::write(src_dir.join("Driver.bd"), "pub i64 Broken() {\n    return true;\n}\n").expect("write sibling");

    with_cwd(&root, || {
        let ctx = CompilationContext::try_for_analysis_path(&entry, None).expect("context");
        let plan = ctx.compile_plan.clone().expect("plan");
        let mod_host = enter_mod_host();
        let native_mods = mod_host.corelib_mod_host(&plan).expect("qualify the native Mods of the compile plan");
        let resolved = resolved_input_from_plan(entry.clone(), source.to_string(), plan, None, None);
        let options = |native_mods: beskid_analysis::mod_host::ModHost| PrepareOptions {
            front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
            ..PrepareOptions::with_mod_host(native_mods)
        };

        let (_, gate, _fixes) =
            beskid_queries::prepare_compilation_diagnostics(&resolved, options(native_mods.clone()), None)
                .expect("analyze");
        let mut editor_db = BeskidDatabase::default();
        let (_, editor, _fixes) = beskid_queries::prepare_compilation_diagnostics_with_db(
            &mut editor_db,
            &resolved,
            options(native_mods),
            None,
        )
        .expect("prepare");

        for (label, diagnostics) in [("command gate", &gate), ("editor database", &editor)] {
            let sibling = diagnostics
                .iter()
                .filter(|diagnostic| {
                    diagnostic.severity == beskid_analysis::Severity::Error && diagnostic.src.name().ends_with("Driver.bd")
                })
                .collect::<Vec<_>>();
            assert!(
                sibling.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("E1206")),
                "{label}: the sibling unit's E1206 must be reported in Driver.bd: {diagnostics:#?}"
            );
            assert!(
                !diagnostics.iter().any(|diagnostic| {
                    diagnostic.severity == beskid_analysis::Severity::Error && diagnostic.src.name().ends_with("Main.bd")
                }),
                "{label}: the healthy entry carries no error: {diagnostics:#?}"
            );
        }
        assert_eq!(diagnostic_codes(&gate), diagnostic_codes(&editor), "both databases report the same codes");
    });

    let _ = fs::remove_dir_all(root);
}
