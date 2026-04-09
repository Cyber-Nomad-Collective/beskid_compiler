use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use beskid_analysis::projects::UnresolvedDependencyPolicy;
use beskid_analysis::services::{resolve_project, resolve_project_with_policy};

fn temp_case_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time ok")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "beskid_projects_resolution_{name}_{}_{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn write_project_manifest(project_dir: &PathBuf, name: &str) {
    let manifest = project_dir.join("Project.proj");
    fs::write(
        manifest,
        format!(
            "project {{\n  name = \"{name}\"\n  version = \"0.1.0\"\n}}\n\ntarget \"App\" {{\n  kind = \"App\"\n  entry = \"Main.bd\"\n}}\n"
        ),
    )
    .expect("write project manifest");
}

#[test]
fn resolve_project_uses_workspace_member_for_input_path() {
    let root = temp_case_dir("workspace_member_from_input");
    let compiler_dir = root.join("compiler");
    let compiler_src = compiler_dir.join("Src");
    fs::create_dir_all(&compiler_src).expect("create compiler src");

    fs::write(
        root.join("Workspace.proj"),
        "workspace {\n  name = \"Root\"\n}\n\nmember \"compiler\" {\n  path = \"compiler\"\n}\n",
    )
    .expect("write workspace");
    write_project_manifest(&compiler_dir, "Compiler");
    fs::write(compiler_src.join("Main.bd"), "fn Main() {}\n").expect("write entry source");

    let input = compiler_src.join("Main.bd");
    let resolved =
        resolve_project(Some(&input), None, None, None, false, false).expect("resolve project");

    let compile_plan = resolved.compile_plan.expect("compile plan present");
    assert_eq!(compile_plan.project_name, "Compiler");
    assert_eq!(
        compile_plan.manifest_path,
        compiler_dir.join("Project.proj")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn resolve_project_with_warn_policy_allows_unresolved_registry_dependencies() {
    let root = temp_case_dir("resolve_project_warn_unresolved_registry");
    let app_dir = root.join("app");
    let app_src = app_dir.join("Src");
    fs::create_dir_all(&app_src).expect("create app src");

    fs::write(
        root.join("Workspace.proj"),
        "workspace {\n  name = \"Root\"\n}\n\nmember \"app\" {\n  path = \"app\"\n}\n\nregistry \"default\" {\n  url = \"https://pckg.beskid-lang.org\"\n}\n",
    )
    .expect("write workspace");
    fs::write(
        app_dir.join("Project.proj"),
        "project {\n  name = \"App\"\n  version = \"0.1.0\"\n}\n\ntarget \"App\" {\n  kind = \"App\"\n  entry = \"Main.bd\"\n}\n\ndependency \"PkgCore\" {\n  source = \"registry\"\n  version = \"1.2.3\"\n  registry = \"default\"\n}\n",
    )
    .expect("write project manifest");
    fs::write(app_src.join("Main.bd"), "fn Main() {}\n").expect("write entry source");

    let workspace_manifest = root.join("Workspace.proj");
    let resolved = resolve_project_with_policy(
        None,
        Some(&workspace_manifest),
        None,
        Some("app"),
        false,
        false,
        UnresolvedDependencyPolicy::Warn,
    )
    .expect("warn policy should allow unresolved registry dependencies");

    let compile_plan = resolved.compile_plan.expect("compile plan present");
    assert_eq!(compile_plan.project_name, "App");
    assert_eq!(compile_plan.unresolved_dependencies.len(), 1);
    assert_eq!(
        compile_plan.unresolved_dependencies[0].dependency_name,
        "PkgCore"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn resolve_project_with_workspace_manifest_uses_first_member_when_no_input() {
    let root = temp_case_dir("workspace_member_default");
    let alpha_dir = root.join("alpha");
    let alpha_src = alpha_dir.join("Src");
    fs::create_dir_all(&alpha_src).expect("create alpha src");

    fs::write(
        root.join("Workspace.proj"),
        "workspace {\n  name = \"Root\"\n}\n\nmember \"alpha\" {\n  path = \"alpha\"\n}\n",
    )
    .expect("write workspace");
    write_project_manifest(&alpha_dir, "Alpha");
    fs::write(alpha_src.join("Main.bd"), "fn Main() {}\n").expect("write entry source");

    let workspace_manifest = root.join("Workspace.proj");
    let resolved = resolve_project(None, Some(&workspace_manifest), None, None, false, false)
        .expect("resolve project");

    let compile_plan = resolved.compile_plan.expect("compile plan present");
    assert_eq!(compile_plan.project_name, "Alpha");
    assert_eq!(compile_plan.manifest_path, alpha_dir.join("Project.proj"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn resolve_project_prefers_deepest_matching_workspace_member() {
    let root = temp_case_dir("workspace_member_deepest_match");
    let tools_dir = root.join("tools");
    let cli_dir = tools_dir.join("cli");
    let tools_src = tools_dir.join("Src");
    let cli_src = cli_dir.join("Src");
    fs::create_dir_all(&tools_src).expect("create tools src");
    fs::create_dir_all(&cli_src).expect("create cli src");

    fs::write(
        root.join("Workspace.proj"),
        "workspace {\n  name = \"Root\"\n}\n\nmember \"tools\" {\n  path = \"tools\"\n}\n\nmember \"cli\" {\n  path = \"tools/cli\"\n}\n",
    )
    .expect("write workspace");

    write_project_manifest(&tools_dir, "Tools");
    write_project_manifest(&cli_dir, "Cli");
    fs::write(cli_src.join("Main.bd"), "fn Main() {}\n").expect("write entry source");

    let input = cli_src.join("Main.bd");
    let resolved =
        resolve_project(Some(&input), None, None, None, false, false).expect("resolve project");

    let compile_plan = resolved.compile_plan.expect("compile plan present");
    assert_eq!(compile_plan.project_name, "Cli");
    assert_eq!(compile_plan.manifest_path, cli_dir.join("Project.proj"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn resolve_project_uses_explicit_workspace_member() {
    let root = temp_case_dir("workspace_member_explicit");
    let alpha_dir = root.join("alpha");
    let beta_dir = root.join("beta");
    let alpha_src = alpha_dir.join("Src");
    let beta_src = beta_dir.join("Src");
    fs::create_dir_all(&alpha_src).expect("create alpha src");
    fs::create_dir_all(&beta_src).expect("create beta src");

    fs::write(
        root.join("Workspace.proj"),
        "workspace {\n  name = \"Root\"\n}\n\nmember \"alpha\" {\n  path = \"alpha\"\n}\n\nmember \"beta\" {\n  path = \"beta\"\n}\n",
    )
    .expect("write workspace");
    write_project_manifest(&alpha_dir, "Alpha");
    write_project_manifest(&beta_dir, "Beta");
    fs::write(beta_src.join("Main.bd"), "fn Main() {}\n").expect("write beta entry source");

    let workspace_manifest = root.join("Workspace.proj");
    let resolved = resolve_project(
        None,
        Some(&workspace_manifest),
        None,
        Some("beta"),
        false,
        false,
    )
    .expect("resolve project");

    let compile_plan = resolved.compile_plan.expect("compile plan present");
    assert_eq!(compile_plan.project_name, "Beta");
    assert_eq!(compile_plan.manifest_path, beta_dir.join("Project.proj"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn resolve_project_errors_for_unknown_workspace_member() {
    let root = temp_case_dir("workspace_member_unknown");
    let alpha_dir = root.join("alpha");
    let alpha_src = alpha_dir.join("Src");
    fs::create_dir_all(&alpha_src).expect("create alpha src");

    fs::write(
        root.join("Workspace.proj"),
        "workspace {\n  name = \"Root\"\n}\n\nmember \"alpha\" {\n  path = \"alpha\"\n}\n",
    )
    .expect("write workspace");
    write_project_manifest(&alpha_dir, "Alpha");
    fs::write(alpha_src.join("Main.bd"), "fn Main() {}\n").expect("write alpha entry source");

    let workspace_manifest = root.join("Workspace.proj");
    let result = resolve_project(
        None,
        Some(&workspace_manifest),
        None,
        Some("missing"),
        false,
        false,
    );
    assert!(result.is_err());
    let message = result.err().map(|err| err.to_string()).unwrap_or_default();
    assert!(message.contains("could not resolve member"));

    let _ = fs::remove_dir_all(root);
}
