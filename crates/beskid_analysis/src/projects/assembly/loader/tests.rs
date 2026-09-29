use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use super::trusted_paths::trusted_corelib_service_paths;
use crate::projects::{
    AssemblyDiscovery, AssemblyError, AssemblyOptions, CompilePlan, EffectiveCompilationRoots,
    ResolvedDependencyProject, RootEntry, Target, TargetKind, assemble_program, assembly_options_for_plan,
    assembly_options_for_prepare, plan_entry_path,
};
use crate::projects::{MaterializedDependencyProject, PreparedProjectWorkspace, SourceUnit};
use crate::services::parse_program_with_source_name;

fn temp_project_root(label: &str) -> PathBuf {
    static NEXT_TEMP_ROOT: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let nonce = NEXT_TEMP_ROOT.fetch_add(1, Ordering::Relaxed);
    let process = std::process::id();
    std::env::temp_dir().join(format!("beskid_asm_{label}_{process}_{nanos}_{nonce}"))
}

fn write_bd(root: &Path, relative: &str, source: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent dirs");
    }
    fs::write(path, source).expect("write bd source");
}

#[cfg(any(unix, windows))]
fn assert_symlink_entry_origin(discovery: AssemblyDiscovery) {
    let disk_source = "i32 Main() { return 0; }";
    let entry_source = "i32 Main() { return 7; }";
    let (mut plan, _) = no_entry_plan_with_source(disk_source);
    let real = plan.source_root.join("Main.bd");
    let alias = plan.source_root.join("Alias.bd");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&real, &alias).expect("create entry symlink");
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&real, &alias).expect("create entry symlink");
    plan.target.entry = Some("Alias.bd".into());
    let options = AssemblyOptions { discovery, ..AssemblyOptions::default() };

    let assembly = assemble_program(&plan, None, &alias, Some(entry_source), &options, None).expect("assemble alias");
    let unit = assembly.entry_unit();
    assert_eq!(unit.origin_path, alias);
    assert_eq!(unit.path, fs::canonicalize(&real).unwrap());
    assert_eq!(unit.logical_name, alias.display().to_string());
    assert_eq!(unit.source, entry_source, "caller entry text must still override disk text");
    assert_eq!(assembly.units.len(), 1, "real and alias remain one physical unit");
    let _ = fs::remove_dir_all(plan.project_root);
}

#[cfg(any(unix, windows))]
#[test]
fn import_closure_retains_symlink_entry_origin() {
    assert_symlink_entry_origin(AssemblyDiscovery::ImportClosure);
}

#[cfg(any(unix, windows))]
#[test]
fn workspace_scan_retains_symlink_entry_origin() {
    assert_symlink_entry_origin(AssemblyDiscovery::WorkspaceScan);
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
        origin_path: materialized_path.clone(),
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
    let roots = crate::projects::effective_roots_for_plan(&plan, Some(&workspace));
    assert_eq!(
        trusted_corelib_service_paths(&plan, &roots, std::slice::from_ref(&unit)),
        Arc::from([materialized_path.clone()]),
        "the materialized path keeps the compiler-owned Foundation origin"
    );

    let mut copied_plan = plan.clone();
    copied_plan.dependency_projects[0].source_root = workspace_root.join("copied/src");
    assert!(
        trusted_corelib_service_paths(&copied_plan, &roots, std::slice::from_ref(&unit)).is_empty(),
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
        origin_path: materialized_path.clone(),
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
    let roots = crate::projects::effective_roots_for_plan(&plan, Some(&workspace));
    assert_eq!(
        trusted_corelib_service_paths(&plan, &roots, std::slice::from_ref(&unit)),
        Arc::from([materialized_path]),
        "resolved Foundation source_root must retain Assert panic provenance"
    );
    let _ = fs::remove_dir_all(workspace_root);
}

#[test]
fn verified_installed_corelib_bundle_preserves_slice_service_provenance() {
    let source = beskid_abi::runtime_source::canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == "Core/Bytes/Slice.bd")
        .expect("embedded Foundation Slice source");
    let identity = beskid_abi::runtime_source::corelib_service_source_identity(&source.logical_path)
        .expect("compiler-owned Slice path");
    let canonical_source_root = identity.canonical_path.ancestors().nth(3).expect("Foundation source root");
    let relative = identity.canonical_path.strip_prefix(canonical_source_root).expect("Slice below source root");
    let project_root = temp_project_root("installed_corelib_bundle");
    let bundle_root = project_root.join("installed/beskid_corelib");
    let foundation_root = bundle_root.join("packages/foundation");
    let source_root = foundation_root.join("src");
    let installed_source = source_root.join(relative);
    write_bd(&source_root, relative.to_str().unwrap(), &source.source);
    write_bd(&foundation_root, "foundation.bproj", "name = \"corelib_foundation\"\n");

    let fingerprint = test_bundle_fingerprint(&bundle_root);
    fs::write(bundle_root.join(".beskid-bundle.sha256"), format!("{fingerprint}\n"))
        .expect("write complete bundle fingerprint");

    let materialized_source_root = project_root.join("obj/beskid/deps/src/corelib_foundation/src");
    let materialized_source = materialized_source_root.join(relative);
    write_bd(&materialized_source_root, relative.to_str().unwrap(), &source.source);
    let unit = SourceUnit {
        logical_name: source.logical_path,
        origin_path: materialized_source.clone(),
        path: materialized_source.canonicalize().expect("physical materialized Slice path"),
        source: source.source.clone(),
        program: parse_program_with_source_name("installed Slice", &source.source).expect("parse Slice source"),
    };
    let plan = CompilePlan {
        project_root: project_root.clone(),
        manifest_path: project_root.join("App.bproj"),
        project_name: "App".into(),
        source_root: project_root.join("src"),
        target: Target { name: "App".into(), kind: TargetKind::App, entry: Some("Main.bd".into()) },
        dependency_projects: vec![ResolvedDependencyProject {
            dependency_name: "corelib_foundation".into(),
            manifest_path: foundation_root.join("foundation.bproj"),
            project_root: foundation_root,
            project_name: "corelib_foundation".into(),
            source_root,
        }],
        unresolved_dependencies: Vec::new(),
        has_std_dependency: true,
    };
    let roots = EffectiveCompilationRoots {
        host: RootEntry { dependency_name: None, source_root: project_root.join("obj/beskid/root/src") },
        dependencies: vec![RootEntry {
            dependency_name: Some("corelib_foundation".into()),
            source_root: materialized_source_root.clone(),
        }],
    };

    assert_eq!(
        trusted_corelib_service_paths(&plan, &roots, std::slice::from_ref(&unit)),
        Arc::from([materialized_source.clone()]),
        "the verified installed bundle must preserve compiler-owned Slice authority after materialization"
    );

    let copied_project = project_root.join("user-copy/packages/foundation");
    let copied_source_root = copied_project.join("src");
    write_bd(&copied_source_root, relative.to_str().unwrap(), &source.source);
    write_bd(&copied_project, "foundation.bproj", "name = \"corelib_foundation\"\n");
    let mut copied_plan = plan.clone();
    copied_plan.dependency_projects[0].project_root = copied_project.clone();
    copied_plan.dependency_projects[0].manifest_path = copied_project.join("foundation.bproj");
    copied_plan.dependency_projects[0].source_root = copied_source_root;
    assert!(
        trusted_corelib_service_paths(&copied_plan, &roots, std::slice::from_ref(&unit)).is_empty(),
        "a user copy of Slice without the verified installed bundle cannot gain service provenance"
    );

    fs::remove_file(&materialized_source).expect("remove materialized Slice before symlinking");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&installed_source, &materialized_source)
        .expect("replace materialized Slice with symlink");
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&installed_source, &materialized_source)
        .expect("replace materialized Slice with symlink");
    let symlink_unit =
        SourceUnit { path: materialized_source.canonicalize().expect("resolve symlink target"), ..unit.clone() };
    assert!(
        trusted_corelib_service_paths(&plan, &roots, std::slice::from_ref(&symlink_unit)).is_empty(),
        "a symlinked materialized Slice cannot inherit compiler service provenance"
    );
    fs::remove_file(&materialized_source).expect("remove materialized Slice symlink");
    write_bd(&materialized_source_root, relative.to_str().unwrap(), &source.source);

    write_bd(&bundle_root, "README.md", "bundle content changed after marker creation\n");
    assert!(
        trusted_corelib_service_paths(&plan, &roots, std::slice::from_ref(&unit)).is_empty(),
        "a stale bundle fingerprint cannot authorize compiler service calls"
    );
    assert!(installed_source.is_file());
    let _ = fs::remove_dir_all(project_root);
}

#[test]
fn verified_installed_corelib_bundle_preserves_assert_service_provenance() {
    let source = beskid_abi::runtime_source::canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == "Testing/Assert.bd")
        .expect("embedded Foundation Assert source");
    let identity = beskid_abi::runtime_source::corelib_service_source_identity(&source.logical_path)
        .expect("compiler-owned Assert path");
    let canonical_source_root = identity.canonical_path.parent().and_then(Path::parent).expect("Foundation src root");
    assert!(canonical_source_root.ends_with("foundation/src"));

    let project_root = temp_project_root("installed_assert_bundle");
    let bundle_root = project_root.join("installed/beskid_corelib");
    let foundation_root = bundle_root.join("packages/foundation");
    let source_root = foundation_root.join("src");
    let relative = Path::new("Testing/Assert.bd");
    write_bd(&source_root, "Testing/Assert.bd", &source.source);
    write_bd(&foundation_root, "foundation.bproj", "name = \"corelib_foundation\"\n");
    let fingerprint = test_bundle_fingerprint(&bundle_root);
    fs::write(bundle_root.join(".beskid-bundle.sha256"), format!("{fingerprint}\n"))
        .expect("write complete bundle fingerprint");

    let materialized_source_root = project_root.join("obj/beskid/deps/src/corelib_foundation/src");
    let materialized_source = materialized_source_root.join(relative);
    write_bd(&materialized_source_root, "Testing/Assert.bd", &source.source);
    let unit = SourceUnit {
        logical_name: source.logical_path,
        origin_path: materialized_source.clone(),
        path: materialized_source.canonicalize().expect("physical materialized Assert path"),
        source: source.source.clone(),
        program: parse_program_with_source_name("installed Assert", &source.source).expect("parse Assert source"),
    };
    let plan = CompilePlan {
        project_root: project_root.clone(),
        manifest_path: project_root.join("App.bproj"),
        project_name: "App".into(),
        source_root: project_root.join("src"),
        target: Target { name: "App".into(), kind: TargetKind::App, entry: Some("Main.bd".into()) },
        dependency_projects: vec![ResolvedDependencyProject {
            dependency_name: "corelib_foundation".into(),
            manifest_path: foundation_root.join("foundation.bproj"),
            project_root: foundation_root,
            project_name: "corelib_foundation".into(),
            source_root,
        }],
        unresolved_dependencies: Vec::new(),
        has_std_dependency: true,
    };
    let roots = EffectiveCompilationRoots {
        host: RootEntry { dependency_name: None, source_root: project_root.join("obj/beskid/root/src") },
        dependencies: vec![RootEntry {
            dependency_name: Some("corelib_foundation".into()),
            source_root: materialized_source_root,
        }],
    };

    assert_eq!(
        trusted_corelib_service_paths(&plan, &roots, std::slice::from_ref(&unit)),
        Arc::from([materialized_source]),
        "the verified installed bundle must preserve compiler-owned Assert authority after materialization"
    );
    let _ = fs::remove_dir_all(project_root);
}

fn test_bundle_fingerprint(root: &Path) -> String {
    fn collect(root: &Path, current: &Path, files: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(current).expect("read bundle directory") {
            let entry = entry.expect("read bundle entry");
            let path = entry.path();
            if path.is_dir() {
                collect(root, &path, files);
            } else if entry.file_name() != ".beskid-bundle.sha256" {
                files.push(path.strip_prefix(root).unwrap().to_path_buf());
            }
        }
    }

    let mut files = Vec::new();
    collect(root, root, &mut files);
    files.sort();
    let mut digest = Sha256::new();
    for relative in files {
        let path = relative.to_string_lossy();
        digest.update((path.len() as u64).to_le_bytes());
        digest.update(path.as_bytes());
        digest.update(fs::read(root.join(relative)).expect("read bundle file"));
    }
    digest.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

fn lock_replayed_foundation_syscall_fixture(label: &str) -> (CompilePlan, PathBuf, SourceUnit) {
    let source = beskid_abi::runtime_source::canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == beskid_abi::runtime_source::CANONICAL_CORELIB_SYSCALL_SOURCE_PATH)
        .expect("embedded Foundation syscall source");
    let canonical_path = beskid_abi::runtime_source::canonical_corelib_service_source_path(&source.logical_path)
        .expect("compiler-owned syscall path");
    let canonical_source_root = canonical_path.ancestors().nth(3).expect("Foundation source root").to_path_buf();
    let canonical_project_root = canonical_source_root.parent().expect("Foundation project root").to_path_buf();
    // Keep the fixture beside this checkout so a Windows TEMP directory on
    // another drive cannot make the declared local path dependency unrelativizable.
    let fixture_name = temp_project_root(label).file_name().expect("temporary fixture name").to_owned();
    let project_root = canonical_project_root.ancestors().nth(3).expect("compiler checkout root").join(fixture_name);
    fs::create_dir_all(&project_root).expect("create replay project");
    let project_root = project_root.canonicalize().expect("physical replay project root");
    let manifest_path = project_root.join("App.bproj");
    write_bd(&project_root, "App.bproj", "name = \"App\"\n");
    write_bd(&project_root, "src/Main.bd", "i32 Main() { return 0; }\n");
    let plan = CompilePlan {
        project_root: project_root.clone(),
        manifest_path: manifest_path.clone(),
        project_name: "App".into(),
        source_root: project_root.join("src"),
        target: Target { name: "App".into(), kind: TargetKind::App, entry: Some("Main.bd".into()) },
        dependency_projects: vec![ResolvedDependencyProject {
            dependency_name: "corelib_foundation".into(),
            manifest_path: canonical_project_root.join("corelib_foundation.bproj"),
            project_root: canonical_project_root.clone(),
            project_name: "corelib_foundation".into(),
            source_root: canonical_source_root.clone(),
        }],
        unresolved_dependencies: Vec::new(),
        has_std_dependency: false,
    };
    // Use the production workspace writer so this is a real, graph-derived v2
    // lock, including its portable source identity and stable destination ID.
    let workspace = crate::projects::prepare_project_workspace(&plan).expect("prepare replay workspace");
    let lockfile = fs::read_to_string(&workspace.lockfile_path).expect("read generated replay lockfile");
    crate::projects::ProjectLockfileV2::parse_v2(&lockfile).expect("parse generated v2 lockfile");
    assert!(lockfile.starts_with("# Project.lock v2\n"));
    assert_eq!(lockfile.lines().filter(|line| line.starts_with("- name=")).count(), 1);
    let materialized_source_root = &workspace.materialized_dependencies[0].materialized_source_root;
    let destination = materialized_source_root.join("Core/Syscall/Syscall.bd");
    assert!(destination.is_file(), "the writer must materialize the canonical Foundation source");
    let unit = SourceUnit::bind_request(
        destination.clone(),
        source.logical_path,
        source.source.clone(),
        parse_program_with_source_name("materialized syscall", &source.source).expect("parse syscall source"),
    );
    (plan, destination, unit)
}

fn lock_replay_field<'a>(lockfile: &'a str, field: &str) -> &'a str {
    let line = lockfile.lines().find(|line| line.starts_with("- name=")).expect("one dependency line");
    line.strip_prefix("- ")
        .expect("dependency prefix")
        .split(';')
        .find_map(|part| part.split_once('=').filter(|(key, _)| *key == field).map(|(_, value)| value))
        .expect("requested dependency field")
}

#[test]
fn valid_lock_replay_trusts_exact_materialized_foundation_source() {
    let (plan, destination, unit) = lock_replayed_foundation_syscall_fixture("valid_service_replay");
    let roots = crate::projects::effective_roots_for_plan(&plan, None);
    assert_eq!(roots.dependencies[0].source_root, destination.parent().unwrap().parent().unwrap().parent().unwrap());
    assert_eq!(
        trusted_corelib_service_paths(&plan, &roots, std::slice::from_ref(&unit)),
        Arc::from([destination]),
        "validated lock replay must preserve the loader-issued Foundation destination"
    );
    let _ = fs::remove_dir_all(plan.project_root);
}

#[test]
fn lock_replay_outside_materialized_dependencies_cannot_grant_service_authority() {
    let (plan, destination, unit) = lock_replayed_foundation_syscall_fixture("rejected_service_replay");
    let untampered_roots = crate::projects::effective_roots_for_plan(&plan, None);
    assert_eq!(
        trusted_corelib_service_paths(&plan, &untampered_roots, std::slice::from_ref(&unit)),
        Arc::from([destination.clone()]),
        "the untampered v2 lock must grant exactly its canonical replay destination"
    );
    fs::create_dir_all(plan.project_root.join("outside")).expect("create outside replay root");
    let lock_path = plan.project_root.join("Project.lock");
    let lockfile = fs::read_to_string(&lock_path).expect("read replay lockfile");
    let lockfile = lockfile.replace(
        &format!("materialized_root={}", lock_replay_field(&lockfile, "materialized_root")),
        "materialized_root=outside",
    );
    fs::write(&lock_path, lockfile).expect("write tampered replay lockfile");
    let roots = crate::projects::effective_roots_for_plan(&plan, None);
    assert_eq!(roots.dependencies[0].source_root, plan.dependency_projects[0].source_root);
    assert!(
        trusted_corelib_service_paths(&plan, &roots, std::slice::from_ref(&unit)).is_empty(),
        "a lockfile path outside materialized dependencies cannot grant Foundation service authority"
    );
    assert!(destination.is_file());
    let _ = fs::remove_dir_all(plan.project_root);
}

#[test]
fn lock_replay_with_forged_dependency_identity_cannot_grant_service_authority() {
    for forged_field in ["manifest", "project_and_source_root"] {
        let (plan, destination, unit) = lock_replayed_foundation_syscall_fixture(forged_field);
        let untampered_roots = crate::projects::effective_roots_for_plan(&plan, None);
        assert_eq!(
            trusted_corelib_service_paths(&plan, &untampered_roots, std::slice::from_ref(&unit)),
            Arc::from([destination.clone()]),
            "the untampered v2 lock must grant exactly its canonical replay destination"
        );
        let lock_path = plan.project_root.join("Project.lock");
        let lockfile = fs::read_to_string(&lock_path).expect("read replay lockfile");
        let dependency = &plan.dependency_projects[0];
        let forged = match forged_field {
            "manifest" => lockfile.replace(
                &format!(";manifest={};", lock_replay_field(&lockfile, "manifest")),
                ";manifest=src/Core/Syscall/Syscall.bd;",
            ),
            "project_and_source_root" => {
                let copied_project = plan.project_root.join("copied");
                fs::create_dir_all(copied_project.join("src")).expect("create forged source root");
                write_bd(&copied_project, "foundation.bproj", "name = \"corelib_foundation\"\n");
                write_bd(&copied_project, "src/Core/Syscall/Syscall.bd", &unit.source);
                lockfile.replace(&format!(";project={};", lock_replay_field(&lockfile, "project")), ";project=copied;")
            }
            _ => unreachable!(),
        };
        assert_ne!(forged, lockfile, "fixture must change the lockfile identity");
        crate::projects::ProjectLockfileV2::parse_v2(&forged).expect("forged identity keeps valid v2 syntax");
        fs::write(&lock_path, forged).expect("write forged replay lockfile");

        let roots = crate::projects::effective_roots_for_plan(&plan, None);
        assert_eq!(roots.dependencies[0].source_root, dependency.source_root, "{forged_field}");
        assert!(
            trusted_corelib_service_paths(&plan, &roots, std::slice::from_ref(&unit)).is_empty(),
            "a forged {forged_field} must not grant service authority"
        );
        assert!(destination.is_file());
        let _ = fs::remove_dir_all(plan.project_root);
    }
}

#[test]
fn lock_replay_with_foreign_project_identity_cannot_grant_service_authority() {
    for forged_field in ["root_manifest", "project_name"] {
        let (plan, destination, unit) = lock_replayed_foundation_syscall_fixture(forged_field);
        let untampered_roots = crate::projects::effective_roots_for_plan(&plan, None);
        assert_eq!(
            trusted_corelib_service_paths(&plan, &untampered_roots, std::slice::from_ref(&unit)),
            Arc::from([destination]),
            "the untampered v2 lock must grant exactly its canonical replay destination"
        );
        let lock_path = plan.project_root.join("Project.lock");
        let lockfile = fs::read_to_string(&lock_path).expect("read replay lockfile");
        let forged = match forged_field {
            "root_manifest" => {
                write_bd(&plan.project_root, "Other.bproj", "name = \"Other\"\n");
                lockfile.replace("root_manifest=App.bproj", "root_manifest=Other.bproj")
            }
            "project_name" => lockfile.replace("project_name=App", "project_name=Other"),
            _ => unreachable!(),
        };
        assert_ne!(forged, lockfile, "fixture must change the root identity");
        crate::projects::ProjectLockfileV2::parse_v2(&forged).expect("foreign identity keeps valid v2 syntax");
        fs::write(&lock_path, forged).expect("write foreign lockfile");

        let roots = crate::projects::effective_roots_for_plan(&plan, None);
        assert_eq!(roots.dependencies[0].source_root, plan.dependency_projects[0].source_root, "{forged_field}");
        assert!(
            trusted_corelib_service_paths(&plan, &roots, std::slice::from_ref(&unit)).is_empty(),
            "a foreign {forged_field} must not grant service authority"
        );
        let _ = fs::remove_dir_all(plan.project_root);
    }
}

fn no_entry_plan_with_source(source: &str) -> (CompilePlan, PathBuf) {
    let project_root = temp_project_root("test");
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
    let paths = super::scanner::module_paths_from_qualified_references(source);
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
