//! Filesystem helpers for the `sample_mod` fixture used by end-to-end mod-host tests.
//!
//! Each test gets an isolated workspace that copies (or synthesizes) the fixture
//! tree under a unique temp dir so descriptors and registrations can be tweaked
//! without leaking across tests.

use std::fs;
use std::path::PathBuf;

use beskid_analysis::projects::{
    effective_roots_from_lockfile, CompilePlan, ResolvedDependencyProject, Target, TargetKind,
};

use beskid_tests_support::temp_case_dir;

const HOST_PROJECT_MANIFEST: &str = "Host.bproj";
const SAMPLE_MOD_PROJECT_MANIFEST: &str = "SampleMod.bproj";
const SAMPLE_MOD_PROJECT: &str = include_str!("../../fixtures/mods/sample_mod/SampleMod.bproj");
const SAMPLE_MOD_SOURCE: &str = include_str!("../../fixtures/mods/sample_mod/Src/Mod.bd");

#[test]
fn sample_mod_materialized_foundation_replays_no_lossy_utf8_append_route() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/mods/sample_mod");
    let lock = fs::read_to_string(fixture.join("Project.lock")).expect("read fixture lockfile");
    let lock_entries: Vec<_> = lock.lines().filter(|line| line.starts_with("- name=")).collect();
    let lock_entry = lock
        .lines()
        .find(|line| line.starts_with("- name=corelib_foundation;"))
        .expect("foundation lock entry");
    let materialized_root = PathBuf::from(lock_entry_field(lock_entry, "materialized_root="));
    let expected_materialized_root = fixture.join(&materialized_root);
    let expected_dependencies_root = fixture.join("obj/beskid/deps/src");
    let expected_source_root = expected_materialized_root.join("src");
    assert!(materialized_root.is_relative(), "checked-in fixture roots must relocate with their worktree");
    assert_eq!(
        expected_materialized_root.parent(),
        Some(expected_dependencies_root.as_path()),
        "lockfile must name a project-relative checked-in fixture root, not merely a basename"
    );
    assert!(
        expected_materialized_root.is_dir(),
        "lock must replay a checked-in materialized foundation snapshot: {lock_entry}"
    );

    let plan = CompilePlan {
        project_root: fixture.clone(),
        manifest_path: fixture.join("SampleMod.bproj"),
        project_name: "SampleMod".to_string(),
        source_root: fixture.join("Src"),
        target: Target { name: "main".to_string(), kind: TargetKind::App, entry: Some("Mod.bd".to_string()) },
        dependency_projects: lock_entries
            .iter()
            .map(|entry| ResolvedDependencyProject {
                dependency_name: lock_entry_field(entry, "name=").to_string(),
                manifest_path: PathBuf::from(lock_entry_field(entry, "manifest=")),
                project_root: PathBuf::from(lock_entry_field(entry, "project=")),
                project_name: lock_entry_field(entry, "name=").to_string(),
                source_root: PathBuf::from(lock_entry_field(entry, "source_root=")),
            })
            .collect(),
        unresolved_dependencies: Vec::new(),
        has_std_dependency: false,
    };
    let replayed = effective_roots_from_lockfile(&plan, &fixture.join("Project.lock"));
    assert_eq!(
        replayed
            .dependencies
            .iter()
            .find(|dependency| dependency.dependency_name.as_deref() == Some("corelib_foundation"))
            .map(|dependency| dependency.source_root.as_path()),
        Some(expected_source_root.as_path()),
        "LSP lockfile replay must resolve the exact local root named by the relative lock value"
    );

    for source in ["String.bd", "Utf8.bd"] {
        let source = fs::read_to_string(
            expected_materialized_root
                .join("src/Core/String")
                .join(source),
        )
        .expect("read materialized Core.String source");
        assert!(
            !source.contains("AppendUtf8Rune"),
            "materialized corelib must not reintroduce the removed lossy UTF-8 append route"
        );
    }
}

fn lock_entry_field<'a>(entry: &'a str, name: &str) -> &'a str {
    entry
        .strip_prefix("- ")
        .unwrap_or(entry)
        .split(';')
        .find_map(|field| field.strip_prefix(name))
        .expect("lock entry field")
}

#[test]
fn lockfile_replay_fails_closed_for_invalid_or_untrusted_entries() {
    let case = ReplayLockCase::new("lock_replay_reject");
    let base = beskid_analysis::projects::effective_roots_from_plan_and_workspace(&case.plan, None);
    let valid = case.entry(&case.relative_materialized_root());
    let invalid_lockfiles = vec![
        "# Project.lock v0\n".to_string(),
        format!("# Project.lock v1\nroot_manifest=x\nproject_name=x\nnot-a-dependency\ndependencies:\n{valid}\n"),
        "# Project.lock v1\nroot_manifest=x\nproject_name=x\ndependencies:\n- name=foundation;manifest=x\n".to_string(),
        format!("{}{}", case.lock_with(&valid), valid),
        case.lock_with(&case.entry(&format!("{}/../escape", case.project.display()))),
        case.lock_with(&case.entry("../outside")),
    ];

    for lock in invalid_lockfiles {
        fs::write(&case.lockfile, &lock).expect("write invalid lockfile");
        assert_eq!(
            effective_roots_from_lockfile(&case.plan, &case.lockfile),
            base,
            "invalid or untrusted lockfile must not partially override roots: {lock}"
        );
    }
}

#[test]
fn lockfile_replay_accepts_contained_absolute_materialized_root() {
    let case = ReplayLockCase::new("lock_replay_absolute");
    fs::write(&case.lockfile, case.lock_with(&case.entry(&case.materialized.display().to_string())))
        .expect("write absolute lockfile");

    let replayed = effective_roots_from_lockfile(&case.plan, &case.lockfile);
    assert_eq!(replayed.dependencies[0].source_root, case.materialized.join("src").canonicalize().expect("canonical source"));
}

struct ReplayLockCase {
    root: PathBuf,
    project: PathBuf,
    materialized: PathBuf,
    lockfile: PathBuf,
    plan: CompilePlan,
}

impl ReplayLockCase {
    fn new(prefix: &str) -> Self {
        let root = temp_case_dir(prefix);
        let project = root.join("App");
        let dependency = root.join("dependency");
        let materialized = project.join("obj/beskid/deps/src/foundation");
        fs::create_dir_all(project.join("Src")).expect("project source root");
        fs::create_dir_all(dependency.join("Src")).expect("dependency source root");
        fs::create_dir_all(materialized.join("src")).expect("materialized source root");
        let plan = CompilePlan {
            project_root: project.clone(),
            manifest_path: project.join("App.bproj"),
            project_name: "App".to_string(),
            source_root: project.join("Src"),
            target: Target { name: "main".to_string(), kind: TargetKind::App, entry: Some("Main.bd".to_string()) },
            dependency_projects: vec![ResolvedDependencyProject {
                dependency_name: "foundation".to_string(),
                manifest_path: dependency.join("Foundation.bproj"),
                project_root: dependency.clone(),
                project_name: "foundation".to_string(),
                source_root: dependency.join("Src"),
            }],
            unresolved_dependencies: Vec::new(),
            has_std_dependency: false,
        };
        Self { lockfile: project.join("Project.lock"), root, project, materialized, plan }
    }

    fn relative_materialized_root(&self) -> String {
        self.materialized.strip_prefix(&self.project).expect("materialized root below project").display().to_string()
    }

    fn entry(&self, materialized_root: &str) -> String {
        let dependency = &self.plan.dependency_projects[0];
        format!(
            "- name=foundation;manifest={};project={};source_root={};materialized_root={materialized_root}\n",
            dependency.manifest_path.display(), dependency.project_root.display(), dependency.source_root.display()
        )
    }

    fn lock_with(&self, entry: &str) -> String {
        format!("# Project.lock v1\nroot_manifest={}\nproject_name=App\ndependencies:\n{entry}", self.plan.manifest_path.display())
    }
}

impl Drop for ReplayLockCase {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// One per-test workspace materialized under `temp_case_dir(prefix)`.
pub(crate) struct ModFixtureWorkspace {
    pub(crate) root: PathBuf,
    pub(crate) host_dir: PathBuf,
    pub(crate) mod_dir: PathBuf,
}

impl ModFixtureWorkspace {
    pub(crate) fn new(prefix: &str) -> Self {
        let root = temp_case_dir(prefix);
        let host_dir = root.join("Host");
        let mod_dir = root.join("SampleMod");
        fs::create_dir_all(host_dir.join("Src")).expect("host source root");
        fs::create_dir_all(mod_dir.join("Src")).expect("mod source root");
        fs::write(host_dir.join("Src").join("Main.bd"), "unit Main() { return; }\n").expect("host source");
        fs::write(host_dir.join(HOST_PROJECT_MANIFEST), HOST_MANIFEST).expect("host manifest");
        fs::write(mod_dir.join(SAMPLE_MOD_PROJECT_MANIFEST), SAMPLE_MOD_PROJECT).expect("mod manifest");
        fs::write(mod_dir.join("Src").join("Mod.bd"), SAMPLE_MOD_SOURCE).expect("mod source");
        Self { root, host_dir, mod_dir }
    }

    pub(crate) fn write_descriptor(&self, registrations_json: &str) -> PathBuf {
        let descriptor_dir = self
            .host_dir
            .join(".beskid")
            .join("obj")
            .join("mods")
            .join("SampleMod")
            .join("cache-key")
            .join("test-triple");
        fs::create_dir_all(&descriptor_dir).expect("descriptor dir");
        let descriptor_path = descriptor_dir.join("mod.descriptor.json");
        let descriptor = format!(
            r#"{{
  "schemaVersion": 1,
  "packageId": "SampleMod",
  "modSourceHash": "fixture-source",
  "lockHash": "fixture-lock",
  "targetTriple": "test-triple",
  "compilerVersion": "test",
  "objectFile": "mod.o",
  "registrations": {registrations_json}
}}"#
        );
        fs::write(&descriptor_path, descriptor).expect("write descriptor");
        descriptor_path
    }

    /// Default registration set covering all four contract kinds plus the
    /// AttributeGenerator surface used by the reference fixture.
    pub(crate) fn default_registrations_json() -> &'static str {
        r#"[
    { "contractId": "Beskid.Compiler.Collect.Collector",          "typeId": "SampleMod.SampleCollect",   "entrySymbol": "samplemod_collect" },
    { "contractId": "Beskid.Compiler.Collect.Generator",          "typeId": "SampleMod.SampleGenerate",  "entrySymbol": "samplemod_generate" },
    { "contractId": "Beskid.Compiler.Collect.AttributeGenerator", "typeId": "SampleMod.SampleAttribute", "entrySymbol": "samplemod_attribute" },
    { "contractId": "Beskid.Compiler.Collect.Analyzer",           "typeId": "SampleMod.SampleAnalyze",   "entrySymbol": "samplemod_analyze" },
    { "contractId": "Beskid.Compiler.Collect.Rewriter",           "typeId": "SampleMod.SampleRewrite",   "entrySymbol": "samplemod_rewrite" }
  ]"#
    }

    pub(crate) fn compile_plan(&self) -> CompilePlan {
        CompilePlan {
            project_root: self.host_dir.clone(),
            manifest_path: self.host_dir.join(HOST_PROJECT_MANIFEST),
            project_name: "Host".to_string(),
            source_root: self.host_dir.join("Src"),
            target: Target { name: "main".to_string(), kind: TargetKind::App, entry: Some("Main.bd".to_string()) },
            dependency_projects: vec![ResolvedDependencyProject {
                dependency_name: "SampleMod".to_string(),
                manifest_path: self.mod_dir.join(SAMPLE_MOD_PROJECT_MANIFEST),
                project_root: self.mod_dir.clone(),
                project_name: "SampleMod".to_string(),
                source_root: self.mod_dir.join("Src"),
            }],
            unresolved_dependencies: Vec::new(),
            has_std_dependency: false,
        }
    }

    pub(crate) fn host_source(&self) -> &'static str {
        "unit Main() { return; }\n"
    }
}

pub(crate) fn typed_items_contain_function(
    items: &[beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Node>],
    name: &str,
) -> bool {
    use beskid_analysis::syntax::Node;
    items.iter().any(|item| {
        matches!(
            &item.node,
            Node::Function(definition) if definition.node.name.node.name == name
        )
    })
}

pub(crate) fn program_contains_function(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    name: &str,
) -> bool {
    typed_items_contain_function(&program.node.items, name)
}

impl Drop for ModFixtureWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

const HOST_MANIFEST: &str = r#"
Host {
  name = "Host"
  version = "0.1.0"
}

target "main" {
  kind = App
  entry = "Main.bd"
}

dependency "SampleMod" {
  source = path
  path = "../SampleMod"
}
"#;
