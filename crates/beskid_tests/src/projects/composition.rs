use std::fs;

use beskid_analysis::CompilationContext;
use beskid_analysis::projects::TargetKind;
use beskid_analysis::services::analyze_source_in_project;

use crate::projects::with_cwd_at_workspace_root;
use crate::test_harness::{temp_case_dir, write_project_manifest as write_manifest};

#[test]
fn app_project_reports_missing_launch_host() {
    let root = temp_case_dir("composition_project_missing_launch");
    let src_dir = root.join("Src");
    fs::create_dir_all(&src_dir).expect("create source root");

    write_manifest(
        &root,
        r#"
project {
  name = "AppWithHost"
  version = "0.1.0"
}

target "app" {
  kind = App
  entry = "Main.bd"
}
"#,
    );

    let source = r#"
host AppHost() : ConsoleHost {
    registry {
        single Logger;
    }
}

i32 Main() {
    return 0;
}
"#;
    let entry = src_dir.join("Main.bd");
    fs::write(&entry, source).expect("write source");

    with_cwd_at_workspace_root(&root, || {
        let context = CompilationContext::try_for_analysis_path(&entry, None)
            .expect("compilation context for project");
        assert_eq!(
            context
                .compile_plan
                .as_ref()
                .expect("compile plan")
                .target
                .kind,
            TargetKind::App
        );
        let diagnostics = analyze_source_in_project(&entry, source)
            .expect("analyze project source");
        let codes: Vec<String> = diagnostics
            .into_iter()
            .filter_map(|diagnostic| diagnostic.code)
            .collect();
        assert!(
            codes.iter().any(|code| code == "E1701"),
            "expected E1701 for app project without launch, got {codes:?}"
        );
    });

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lib_project_reports_launch_in_library_target() {
    let root = temp_case_dir("composition_project_lib_launch");
    let src_dir = root.join("Src");
    fs::create_dir_all(&src_dir).expect("create source root");

    write_manifest(
        &root,
        r#"
project {
  name = "LibWithLaunch"
  version = "0.1.0"
}

target "lib" {
  kind = Lib
  entry = "Lib.bd"
}
"#,
    );

    let source = r#"
host AppHost() : ConsoleHost {
    registry {
        single Logger;
    }
}

i32 marker() {
    launch AppHost();
    return 0;
}
"#;
    let entry = src_dir.join("Lib.bd");
    fs::write(&entry, source).expect("write source");

    with_cwd_at_workspace_root(&root, || {
        let context = CompilationContext::try_for_analysis_path(&entry, None)
            .expect("compilation context for project");
        assert_eq!(
            context
                .compile_plan
                .as_ref()
                .expect("compile plan")
                .target
                .kind,
            TargetKind::Lib
        );
        let diagnostics = analyze_source_in_project(&entry, source)
            .expect("analyze lib project source");
        let codes: Vec<String> = diagnostics
            .into_iter()
            .filter_map(|diagnostic| diagnostic.code)
            .collect();
        assert!(
            codes.iter().any(|code| code == "E1711"),
            "expected E1711 for launch in lib project, got {codes:?}"
        );
    });

    let _ = fs::remove_dir_all(root);
}
