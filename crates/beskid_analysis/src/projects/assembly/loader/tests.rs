use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use super::trusted_paths::trusted_corelib_service_paths;
use crate::projects::{
    assemble_program, assembly_options_for_plan, assembly_options_for_prepare, plan_entry_path, AssemblyDiscovery,
    AssemblyError, AssemblyOptions, CompilePlan, ResolvedDependencyProject, Target, TargetKind,
};
use crate::projects::{MaterializedDependencyProject, PreparedProjectWorkspace, SourceUnit};
use crate::services::parse_program_with_source_name;

fn temp_project_root(label: &str) -> PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    std::env::temp_dir().join(format!("beskid_asm_{label}_{nanos}"))
}

fn write_bd(root: &Path, relative: &str, source: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent dirs");
    }
    fs::write(path, source).expect("write bd source");
}

#[test]
fn materialized_compiler_foundation_path_retains_service_provenance_but_a_copy_does_not() {
    let source = beskid_abi::runtime_source::canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == beskid_abi::runtime_source::CANONICAL_CORELIB_SYSCALL_SOURCE_PATH)
        .expect("embedded Foundation syscall source");
    let canonical_path = beskid_abi::runtime_source::canonical_corelib_service_source_path(&source.logical_path)
        .expect("compiler-owned syscall path");
    let canonical_source_root = canonical_path.ancestors().nth(3).expect("Foundation source root").to_path_buf();
    let canonical_project_root = canonical_source_root.parent().expect("Foundation project root").to_path_buf();
    let workspace_root = temp_project_root("trusted_foundation_materialization");
    let materialized_source_root = workspace_root.join("deps/foundation/src");
    let relative = canonical_path.strip_prefix(&canonical_source_root).expect("syscall below source root");
    let materialized_path = materialized_source_root.join(relative);
    let unit = SourceUnit {
        logical_name: materialized_path.display().to_string(),
        path: materialized_path.clone(),
        source: source.source.clone(),
        program: parse_program_with_source_name("materialized syscall", &source.source).expect("parse syscall source"),
    };
    let plan = CompilePlan {
        project_root: workspace_root.clone(),
        manifest_path: workspace_root.join("App.bproj"),
        project_name: "App".into(),
        source_root: workspace_root.join("src"),
        target: Target { name: "App".into(), kind: TargetKind::App, entry: Some("Main.bd".into()) },
        dependency_projects: vec![ResolvedDependencyProject {
            dependency_name: "corelib_foundation".into(),
            manifest_path: canonical_project_root.join("corelib_foundation.bproj"),
            project_root: canonical_project_root,
            project_name: "corelib_foundation".into(),
            source_root: canonical_source_root,
        }],
        unresolved_dependencies: Vec::new(),
        has_std_dependency: false,
    };
    let workspace = PreparedProjectWorkspace {
        lockfile_path: workspace_root.join("Project.lock"),
        materialized_project_root: workspace_root.join("root"),
        materialized_source_root: workspace_root.join("root/src"),
        materialized_dependencies: vec![MaterializedDependencyProject {
            dependency_name: "corelib_foundation".into(),
            manifest_path: plan.dependency_projects[0].manifest_path.clone(),
            project_name: "corelib_foundation".into(),
            materialized_project_root: workspace_root.join("deps/foundation"),
            materialized_source_root: materialized_source_root.clone(),
        }],
    };
    assert_eq!(
        trusted_corelib_service_paths(&plan, Some(&workspace), std::slice::from_ref(&unit)),
        Arc::from([materialized_path.clone()]),
        "the materialized path keeps the compiler-owned Foundation origin"
    );

    let mut copied_plan = plan.clone();
    copied_plan.dependency_projects[0].source_root = workspace_root.join("copied/src");
    assert!(
        trusted_corelib_service_paths(&copied_plan, Some(&workspace), std::slice::from_ref(&unit),).is_empty(),
        "a copied source root cannot inherit Corelib service provenance"
    );
    let _ = fs::remove_dir_all(workspace_root);
}

#[test]
fn resolved_foundation_source_root_still_trusts_materialized_assert() {
    // Production CompilePlan records a filesystem-resolved Foundation `source_root`
    // (`.../packages/foundation/src`). The compiler-owned path historically retained
    // `../..` from CARGO_MANIFEST_DIR; Path::starts_with then failed and Assert lost
    // panic_str authority under Corelib tests.
    let source = beskid_abi::runtime_source::canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == beskid_abi::runtime_source::CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH)
        .expect("embedded Foundation Assert source");
    let canonical_path = beskid_abi::runtime_source::canonical_corelib_service_source_path(&source.logical_path)
        .expect("compiler-owned Assert path");
    assert!(
        !canonical_path.components().any(|component| matches!(component, std::path::Component::ParentDir)),
        "canonical service paths must be lexically normalized: {canonical_path:?}"
    );
    let canonical_source_root = fs::canonicalize(
        canonical_path.parent().and_then(|testing| testing.parent()).expect("Assert under foundation/src"),
    )
    .expect("resolve foundation source root");
    let canonical_project_root = canonical_source_root.parent().expect("Foundation project root").to_path_buf();
    let workspace_root = temp_project_root("trusted_assert_resolved_root");
    let materialized_source_root = workspace_root.join("deps/foundation/src");
    let relative = canonical_path.strip_prefix(&canonical_source_root).expect("Assert below source root");
    let materialized_path = materialized_source_root.join(relative);
    let unit = SourceUnit {
        logical_name: materialized_path.display().to_string(),
        path: materialized_path.clone(),
        source: source.source.clone(),
        program: parse_program_with_source_name("materialized assert", &source.source).expect("parse Assert source"),
    };
    let plan = CompilePlan {
        project_root: workspace_root.clone(),
        manifest_path: workspace_root.join("App.bproj"),
        project_name: "App".into(),
        source_root: workspace_root.join("src"),
        target: Target { name: "App".into(), kind: TargetKind::App, entry: Some("Main.bd".into()) },
        dependency_projects: vec![ResolvedDependencyProject {
            dependency_name: "corelib_foundation".into(),
            manifest_path: canonical_project_root.join("corelib_foundation.bproj"),
            project_root: canonical_project_root,
            project_name: "corelib_foundation".into(),
            source_root: canonical_source_root,
        }],
        unresolved_dependencies: Vec::new(),
        has_std_dependency: false,
    };
    let workspace = PreparedProjectWorkspace {
        lockfile_path: workspace_root.join("Project.lock"),
        materialized_project_root: workspace_root.join("root"),
        materialized_source_root: workspace_root.join("root/src"),
        materialized_dependencies: vec![MaterializedDependencyProject {
            dependency_name: "corelib_foundation".into(),
            manifest_path: plan.dependency_projects[0].manifest_path.clone(),
            project_name: "corelib_foundation".into(),
            materialized_project_root: workspace_root.join("deps/foundation"),
            materialized_source_root: materialized_source_root.clone(),
        }],
    };
    assert_eq!(
        trusted_corelib_service_paths(&plan, Some(&workspace), std::slice::from_ref(&unit)),
        Arc::from([materialized_path]),
        "resolved Foundation source_root must retain Assert panic provenance"
    );
    let _ = fs::remove_dir_all(workspace_root);
}

fn no_entry_plan_with_source(source: &str) -> (CompilePlan, PathBuf) {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let project_root = std::env::temp_dir().join(format!("beskid_asm_test_{nanos}"));
    let source_root = project_root.join("src");
    fs::create_dir_all(&source_root).expect("create source root");
    fs::write(source_root.join("Main.bd"), source).expect("write Main.bd");
    let plan = CompilePlan {
        source_root: source_root.clone(),
        project_root: project_root.clone(),
        manifest_path: project_root.join("project.bproj"),
        project_name: "fixture".to_string(),
        target: Target { name: "__aggregate__".to_string(), kind: TargetKind::Lib, entry: None },
        dependency_projects: Vec::new(),
        unresolved_dependencies: Vec::new(),
        has_std_dependency: false,
    };
    let entry_path = plan_entry_path(&plan, &source_root);
    (plan, entry_path)
}

#[test]
fn no_entry_plan_uses_workspace_scan_discovery() {
    let (plan, _) = no_entry_plan_with_source("pub fn Main() { }");
    let options = assembly_options_for_plan(&plan);
    assert_eq!(options.discovery, AssemblyDiscovery::WorkspaceScan);
}

#[test]
fn entry_plan_uses_import_closure_discovery() {
    let (mut plan, _) = no_entry_plan_with_source("pub fn Main() { }");
    plan.target.entry = Some("Main.bd".to_string());
    let options = assembly_options_for_plan(&plan);
    assert_eq!(options.discovery, AssemblyDiscovery::ImportClosure);
}

#[test]
fn qualified_reference_scan_finds_module_prefixes() {
    let source = "Core.Results.Result<i64, SyscallError> Write() { Core.Syscall.WriteWith(x); }";
    let paths = super::module_paths_from_qualified_references(source);
    assert!(paths.contains(&"Core.Results".to_string()));
    assert!(paths.contains(&"Core".to_string()));
    assert!(paths.contains(&"Core.Syscall".to_string()));
}

#[test]
fn workspace_scan_assembles_without_placeholder_entry_file() {
    let (plan, entry_path) = no_entry_plan_with_source("pub fn Main() { }");
    let options = assembly_options_for_plan(&plan);
    assert!(!entry_path.is_file(), "placeholder entry should not exist: {}", entry_path.display());

    let assembly = assemble_program(&plan, None, &entry_path, Some(""), &options, None)
        .expect("workspace scan should assemble units without a real entry file");
    assert!(!assembly.units.is_empty());
    assert_eq!(assembly.units.len(), assembly.syntax_indexes.len());
    assert!(assembly.syntax_indexes.iter().all(|index| index.generation() == assembly.generation));
    let _ = fs::remove_dir_all(&plan.project_root);
}

#[test]
fn import_closure_still_requires_entry_file() {
    let (plan, entry_path) = no_entry_plan_with_source("pub fn Main() { }");
    let mut options = assembly_options_for_plan(&plan);
    options.discovery = AssemblyDiscovery::ImportClosure;
    let err = assemble_program(&plan, None, &entry_path, Some(""), &options, None)
        .expect_err("import closure without entry file should fail");
    assert!(matches!(err, AssemblyError::EntryNotFound { .. }), "unexpected error: {err}");
    let _ = fs::remove_dir_all(&plan.project_root);
}

#[test]
fn prepare_options_use_plan_default_when_front_end_is_import_closure() {
    let (plan, _) = no_entry_plan_with_source("pub fn Main() { }");
    let options = assembly_options_for_prepare(&plan, AssemblyDiscovery::ImportClosure);
    assert_eq!(options.discovery, AssemblyDiscovery::WorkspaceScan);

    let mut entry_plan = plan.clone();
    entry_plan.target.entry = Some("Main.bd".to_string());
    let options = assembly_options_for_prepare(&entry_plan, AssemblyDiscovery::ImportClosure);
    assert_eq!(options.discovery, AssemblyDiscovery::ImportClosure);
    let _ = fs::remove_dir_all(&plan.project_root);
}

#[test]
fn prepare_options_honor_explicit_front_end_override() {
    let (mut plan, _) = no_entry_plan_with_source("pub fn Main() { }");
    plan.target.entry = Some("Main.bd".to_string());
    let options = assembly_options_for_prepare(&plan, AssemblyDiscovery::WorkspaceScan);
    assert_eq!(options.discovery, AssemblyDiscovery::WorkspaceScan);
    let _ = fs::remove_dir_all(&plan.project_root);
}

#[test]
fn import_closure_assembles_entry_without_sibling_units() {
    let project_root = temp_project_root("import_closure_entry_only");
    let source_root = project_root.join("src");
    write_bd(&source_root, "Entry.bd", "pub fn Entry() { }");
    write_bd(&source_root, "Sibling.bd", "pub fn Sibling() { }");
    let plan = CompilePlan {
        source_root: source_root.clone(),
        project_root: project_root.clone(),
        manifest_path: project_root.join("project.bproj"),
        project_name: "fixture".to_string(),
        target: Target { name: "Entry".to_string(), kind: TargetKind::Lib, entry: Some("Entry.bd".to_string()) },
        dependency_projects: Vec::new(),
        unresolved_dependencies: Vec::new(),
        has_std_dependency: false,
    };
    let entry_path = source_root.join("Entry.bd");
    let options = assembly_options_for_plan(&plan);
    let assembly =
        assemble_program(&plan, None, &entry_path, None, &options, None).expect("import closure should assemble entry");
    assert_eq!(assembly.units.len(), 1);
    assert_eq!(assembly.discovery, AssemblyDiscovery::ImportClosure);
    assert!(
        assembly.units.iter().all(|unit| unit.path.file_name().and_then(|name| name.to_str()) == Some("Entry.bd")),
        "unexpected units: {:?}",
        assembly.units.iter().map(|unit| unit.path.display().to_string()).collect::<Vec<_>>()
    );
    let _ = fs::remove_dir_all(&project_root);
}

#[test]
fn import_closure_follows_qualified_nominal_references() {
    let project_root = temp_project_root("import_closure_qualified_nominal");
    let source_root = project_root.join("src");
    write_bd(
        &source_root,
        "Entry.bd",
        "pub Console.ConsoleSize Entry() { return Console.ConsoleSize { columns: 80, rows: 24 }; }",
    );
    write_bd(&source_root, "Console/Console.bd", "pub type ConsoleSize { i32 columns, i32 rows }");
    let plan = CompilePlan {
        source_root: source_root.clone(),
        project_root: project_root.clone(),
        manifest_path: project_root.join("project.bproj"),
        project_name: "fixture".to_string(),
        target: Target { name: "Entry".to_string(), kind: TargetKind::Lib, entry: Some("Entry.bd".to_string()) },
        dependency_projects: Vec::new(),
        unresolved_dependencies: Vec::new(),
        has_std_dependency: false,
    };
    let entry_path = source_root.join("Entry.bd");
    let options = assembly_options_for_plan(&plan);
    let assembly = assemble_program(&plan, None, &entry_path, None, &options, None)
        .expect("import closure should follow qualified nominal references");
    assert!(
        assembly.units.iter().any(|unit| unit.path.ends_with("Console/Console.bd")),
        "qualified nominal authority must be assembled: {:?}",
        assembly.units.iter().map(|unit| unit.path.display().to_string()).collect::<Vec<_>>()
    );
    let _ = fs::remove_dir_all(&project_root);
}

#[test]
fn import_closure_follows_transitive_use_imports() {
    let project_root = temp_project_root("import_closure_transitive");
    let source_root = project_root.join("src");
    write_bd(&source_root, "Entry.bd", "use Lib.A;\npub fn Entry() { Lib.A.Run(); }");
    write_bd(&source_root, "Lib/A.bd", "use Lib.B;\npub fn Run() { Lib.B.Run(); }");
    write_bd(&source_root, "Lib/B.bd", "pub fn Run() { }");
    write_bd(&source_root, "Unused.bd", "pub fn Unused() { }");
    let plan = CompilePlan {
        source_root: source_root.clone(),
        project_root: project_root.clone(),
        manifest_path: project_root.join("project.bproj"),
        project_name: "fixture".to_string(),
        target: Target { name: "Entry".to_string(), kind: TargetKind::Lib, entry: Some("Entry.bd".to_string()) },
        dependency_projects: Vec::new(),
        unresolved_dependencies: Vec::new(),
        has_std_dependency: false,
    };
    let entry_path = source_root.join("Entry.bd");
    let options = assembly_options_for_plan(&plan);
    let assembly = assemble_program(&plan, None, &entry_path, None, &options, None)
        .expect("import closure should follow transitive imports");
    let names: Vec<String> =
        assembly.units.iter().map(|unit| unit.path.file_name().unwrap().to_string_lossy().into_owned()).collect();
    assert_eq!(names.len(), 3);
    assert!(names.iter().any(|name| name == "Entry.bd"));
    assert!(names.iter().any(|name| name == "A.bd"));
    assert!(names.iter().any(|name| name == "B.bd"));
    assert!(!names.iter().any(|name| name == "Unused.bd"));
    let _ = fs::remove_dir_all(&project_root);
}

#[test]
fn import_closure_follows_public_module_declarations() {
    let project_root = temp_project_root("import_closure_public_module");
    let source_root = project_root.join("src");
    write_bd(&source_root, "Entry.bd", "use Core.Text.Regex;\npub fn Entry() { Core.Text.Regex.Parse(); }");
    write_bd(
        &source_root,
        "Core/Text/Regex.bd",
        "pub mod Core.Text.Regex.Generated;\npub fn Parse() { Core.Text.Regex.Generated.ParsePat(); }",
    );
    write_bd(&source_root, "Core/Text/Regex/Generated.bd", "pub fn ParsePat() { }");
    let plan = CompilePlan {
        source_root: source_root.clone(),
        project_root: project_root.clone(),
        manifest_path: project_root.join("project.bproj"),
        project_name: "fixture".to_string(),
        target: Target { name: "Entry".to_string(), kind: TargetKind::Lib, entry: Some("Entry.bd".to_string()) },
        dependency_projects: Vec::new(),
        unresolved_dependencies: Vec::new(),
        has_std_dependency: false,
    };
    let assembly =
        assemble_program(&plan, None, &source_root.join("Entry.bd"), None, &assembly_options_for_plan(&plan), None)
            .expect("public module declaration should extend import closure");
    let loaded: Vec<_> = assembly.units.iter().map(|unit| &unit.path).collect();
    assert!(loaded.iter().any(|path| path.ends_with("Entry.bd")), "expected entry in closure, got: {loaded:?}");
    assert!(
        loaded.iter().any(|path| path.ends_with("Core/Text/Regex.bd")),
        "expected declared module owner in closure, got: {loaded:?}"
    );
    assert!(
        loaded.iter().any(|path| path.ends_with("Core/Text/Regex/Generated.bd")),
        "expected declared generated module in closure, got: {loaded:?}"
    );
    let _ = fs::remove_dir_all(&project_root);
}

#[test]
fn import_closure_follows_public_module_declarations_into_generated_sources() {
    let project_root = temp_project_root("import_closure_generated_public_module");
    let source_root = project_root.join("src");
    write_bd(&source_root, "Entry.bd", "use Core.Text.Regex;\npub fn Entry() { Core.Text.Regex.Parse(); }");
    write_bd(
        &source_root,
        "Core/Text/Regex.bd",
        "pub mod Core.Text.Regex.Generated;\npub fn Parse() { Core.Text.Regex.Generated.ParsePat(); }",
    );
    write_bd(&project_root.join(".generated"), "Core/Text/Regex/Generated.g.bd", "pub fn ParsePat() { }");
    let plan = CompilePlan {
        source_root: source_root.clone(),
        project_root: project_root.clone(),
        manifest_path: project_root.join("project.bproj"),
        project_name: "fixture".to_string(),
        target: Target { name: "Entry".to_string(), kind: TargetKind::Lib, entry: Some("Entry.bd".to_string()) },
        dependency_projects: Vec::new(),
        unresolved_dependencies: Vec::new(),
        has_std_dependency: false,
    };
    let assembly =
        assemble_program(&plan, None, &source_root.join("Entry.bd"), None, &assembly_options_for_plan(&plan), None)
            .expect("public module declaration should resolve its generated source");
    let loaded: Vec<_> = assembly.units.iter().map(|unit| &unit.path).collect();
    assert!(
        loaded.iter().any(|path| path.ends_with(".generated/Core/Text/Regex/Generated.g.bd")),
        "expected generated declared module in closure, got: {loaded:?}"
    );
    let _ = fs::remove_dir_all(&project_root);
}

#[test]
fn import_closure_ignores_missing_public_module_declarations() {
    let project_root = temp_project_root("import_closure_missing_public_module");
    let source_root = project_root.join("src");
    write_bd(&source_root, "Entry.bd", "pub mod Core.Text.DoesNotExist;\npub fn Entry() { }");
    let plan = CompilePlan {
        source_root: source_root.clone(),
        project_root: project_root.clone(),
        manifest_path: project_root.join("project.bproj"),
        project_name: "fixture".to_string(),
        target: Target { name: "Entry".to_string(), kind: TargetKind::Lib, entry: Some("Entry.bd".to_string()) },
        dependency_projects: Vec::new(),
        unresolved_dependencies: Vec::new(),
        has_std_dependency: false,
    };
    let assembly =
        assemble_program(&plan, None, &source_root.join("Entry.bd"), None, &assembly_options_for_plan(&plan), None)
            .expect("absent module declaration target should not invalidate existing closure");
    assert_eq!(assembly.units.len(), 1);
    let _ = fs::remove_dir_all(&project_root);
}

#[test]
fn import_closure_terminates_public_module_declaration_cycles() {
    let project_root = temp_project_root("import_closure_public_module_cycle");
    let source_root = project_root.join("src");
    write_bd(&source_root, "Entry.bd", "use Core.A;\npub fn Entry() { }");
    write_bd(&source_root, "Core/A.bd", "pub mod Core.B;\npub fn A() { }");
    write_bd(&source_root, "Core/B.bd", "pub mod Core.A;\npub fn B() { }");
    let plan = CompilePlan {
        source_root: source_root.clone(),
        project_root: project_root.clone(),
        manifest_path: project_root.join("project.bproj"),
        project_name: "fixture".to_string(),
        target: Target { name: "Entry".to_string(), kind: TargetKind::Lib, entry: Some("Entry.bd".to_string()) },
        dependency_projects: Vec::new(),
        unresolved_dependencies: Vec::new(),
        has_std_dependency: false,
    };
    let assembly =
        assemble_program(&plan, None, &source_root.join("Entry.bd"), None, &assembly_options_for_plan(&plan), None)
            .expect("module declaration cycles should be de-duplicated");
    assert_eq!(assembly.units.len(), 3);
    assert!(assembly.module_index.known_module_path_strings().contains("Core::A"));
    assert!(assembly.module_index.known_module_path_strings().contains("Core::B"));
    let _ = fs::remove_dir_all(&project_root);
}

#[test]
fn workspace_scan_assembles_all_host_sources() {
    let project_root = temp_project_root("workspace_scan_all");
    let source_root = project_root.join("src");
    write_bd(&source_root, "Main.bd", "pub fn Main() { }");
    write_bd(&source_root, "Other.bd", "pub fn Other() { }");
    let plan = CompilePlan {
        source_root: source_root.clone(),
        project_root: project_root.clone(),
        manifest_path: project_root.join("project.bproj"),
        project_name: "fixture".to_string(),
        target: Target { name: "__aggregate__".to_string(), kind: TargetKind::Lib, entry: None },
        dependency_projects: Vec::new(),
        unresolved_dependencies: Vec::new(),
        has_std_dependency: false,
    };
    let entry_path = plan_entry_path(&plan, &source_root);
    let options = assembly_options_for_plan(&plan);
    let assembly = assemble_program(&plan, None, &entry_path, Some(""), &options, None)
        .expect("workspace scan should assemble every host unit");
    assert_eq!(assembly.discovery, AssemblyDiscovery::WorkspaceScan);
    assert_eq!(assembly.units.len(), 2);
    let _ = fs::remove_dir_all(&project_root);
}

#[test]
fn import_closure_module_index_skips_unimported_dependency_tree() {
    let project_root = temp_project_root("import_closure_dep_prefetch");
    let source_root = project_root.join("src");
    let dep_root = project_root.join("deps").join("core");
    let dep_source_root = dep_root.join("src");
    write_bd(&source_root, "Entry.bd", "pub fn Entry() { }");
    for index in 0..8 {
        write_bd(&dep_source_root, &format!("Shard{index}.bd"), &format!("pub fn Shard{index}() {{ }}"));
    }
    let plan = CompilePlan {
        source_root: source_root.clone(),
        project_root: project_root.clone(),
        manifest_path: project_root.join("project.bproj"),
        project_name: "fixture".to_string(),
        target: Target { name: "Entry".to_string(), kind: TargetKind::Lib, entry: Some("Entry.bd".to_string()) },
        dependency_projects: vec![ResolvedDependencyProject {
            dependency_name: "core".to_string(),
            manifest_path: dep_root.join("core.bproj"),
            project_root: dep_root.clone(),
            project_name: "core".to_string(),
            source_root: dep_source_root.clone(),
        }],
        unresolved_dependencies: Vec::new(),
        has_std_dependency: false,
    };
    let entry_path = source_root.join("Entry.bd");
    let options = AssemblyOptions { discovery: AssemblyDiscovery::ImportClosure, ..AssemblyOptions::default() };
    let assembly = assemble_program(&plan, None, &entry_path, None, &options, None)
        .expect("import closure should assemble entry without dependency units");
    assert_eq!(assembly.units.len(), 1);
    assert!(
        assembly.module_index.prefetched_paths().is_empty(),
        "expected no dependency prefetch for zero-import entry, got {} paths",
        assembly.module_index.prefetched_paths().len()
    );

    let scan_options = AssemblyOptions { discovery: AssemblyDiscovery::WorkspaceScan, ..AssemblyOptions::default() };
    let scanned = assemble_program(&plan, None, &entry_path, None, &scan_options, None)
        .expect("workspace scan should assemble host and prefetch dependency tree");
    assert!(
        scanned.units.len() >= 9,
        "workspace scan should assemble host and dependency shards as units, got {}",
        scanned.units.len()
    );
    let _ = fs::remove_dir_all(&project_root);
}
