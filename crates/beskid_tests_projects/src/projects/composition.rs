use std::fs;

use beskid_analysis::CompilationContext;
use beskid_analysis::projects::TargetKind;
use crate::projects::fixture_harness::analyze_file_with_mods;

use crate::projects::with_cwd_at_workspace_root;
use beskid_tests_support::{temp_case_dir, write_project_manifest as write_manifest};

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
        let context = CompilationContext::try_for_analysis_path(&entry, None).expect("compilation context for project");
        assert_eq!(context.compile_plan.as_ref().expect("compile plan").target.kind, TargetKind::App);
        let diagnostics = analyze_file_with_mods(&entry).expect("analyze project source");
        let codes: Vec<String> = diagnostics.into_iter().filter_map(|diagnostic| diagnostic.code).collect();
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
        let context = CompilationContext::try_for_analysis_path(&entry, None).expect("compilation context for project");
        assert_eq!(context.compile_plan.as_ref().expect("compile plan").target.kind, TargetKind::Lib);
        let diagnostics = analyze_file_with_mods(&entry).expect("analyze lib project source");
        let codes: Vec<String> = diagnostics.into_iter().filter_map(|diagnostic| diagnostic.code).collect();
        assert!(codes.iter().any(|code| code == "E1711"), "expected E1711 for launch in lib project, got {codes:?}");
    });

    let _ = fs::remove_dir_all(root);
}

/// Analyze one App project whose `Main.bd` is `source` and return its diagnostic codes. The
/// pipeline hands composition the query layer's type authority (`CompositionTypes`), so keys and
/// inject requests resolve as `beskid check` resolves them.
fn app_diagnostic_codes(case: &str, source: &str) -> Vec<String> {
    let root = temp_case_dir(case);
    let src_dir = root.join("Src");
    fs::create_dir_all(&src_dir).expect("create source root");
    write_manifest(
        &root,
        r#"
project {
  name = "ContractKeyedApp"
  version = "0.1.0"
}

target "app" {
  kind = App
  entry = "Main.bd"
}
"#,
    );
    let entry = src_dir.join("Main.bd");
    fs::write(&entry, source).expect("write source");
    let mut codes = Vec::new();
    with_cwd_at_workspace_root(&root, || {
        let diagnostics = analyze_file_with_mods(&entry).expect("analyze project source");
        codes = diagnostics.into_iter().filter_map(|diagnostic| diagnostic.code).collect();
    });
    let _ = fs::remove_dir_all(root);
    codes
}

#[test]
fn contract_keyed_registration_satisfies_inject_of_the_contract() {
    let codes = app_diagnostic_codes(
        "composition_contract_keyed_inject",
        r#"
contract Clock { i64 Now(); }
type SystemClock : Clock { pub i64 Now() { return 1; } }
type Reception { pub i32 Greet() { return 1; } }
type User { inject Clock clock; }
host AppHost()
{
    registry { single SystemClock for Clock; single User; single Reception; }
    startup(Reception reception, Clock clock) { reception.Greet(); clock.Now(); }
}
i32 Main() { launch AppHost(); return 0; }
"#,
    );
    assert!(codes.iter().all(|code| !code.starts_with('E')), "expected no errors, got {codes:?}");
}

#[test]
fn inject_of_an_unregistered_contract_has_no_provider() {
    let codes = app_diagnostic_codes(
        "composition_contract_unregistered_inject",
        r#"
contract Clock { i64 Now(); }
contract Calendar { i64 Today(); }
type SystemClock : Clock { pub i64 Now() { return 1; } }
type User { inject Calendar calendar; }
host AppHost() { registry { single SystemClock for Clock; single User; } }
i32 Main() { launch AppHost(); return 0; }
"#,
    );
    assert!(codes.iter().any(|code| code == "E1704"), "expected E1704 for the unregistered contract, got {codes:?}");
}

const GENERIC_CONTRACT_KEYS: &str = r#"
contract Repo<T> { i64 Count(); }
type User { }
type Order { }
type SqlRepo : Repo<User> { pub i64 Count() { return 0; } }
type Service { inject Repo<REQUESTED> repo; }
host AppHost() { registry { single SqlRepo for Repo<User>; single Service; } }
i32 Main() { launch AppHost(); return 0; }
"#;

#[test]
fn a_generic_contract_key_matches_the_inject_with_the_same_type_arguments() {
    let codes =
        app_diagnostic_codes("composition_generic_contract_key", &GENERIC_CONTRACT_KEYS.replace("REQUESTED", "User"));
    assert!(codes.iter().all(|code| !code.starts_with('E')), "expected no errors, got {codes:?}");
}

#[test]
fn a_generic_contract_key_does_not_match_other_type_arguments() {
    let codes = app_diagnostic_codes(
        "composition_generic_contract_other_arguments",
        &GENERIC_CONTRACT_KEYS.replace("REQUESTED", "Order"),
    );
    assert!(codes.iter().any(|code| code == "E1704"), "`Repo<Order>` is not registered, got {codes:?}");
}
