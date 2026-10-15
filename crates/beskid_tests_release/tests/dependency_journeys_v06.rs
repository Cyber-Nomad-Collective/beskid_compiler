//! DEP06-01..05: transactional dependency journeys of the installed CLI against a loopback registry
//! (`tooling--manifests-and-lockfiles--workspace-and-lock-contracts`,
//! `compiler--resolution-and-projects--registry-and-overrides-contract`,
//! `compiler--resolution-and-projects--workspace-and-lock-contracts`).
use beskid_tests_release::installed::{Cli, Home, transcript};
use beskid_tests_release::project::{lock_pin, snapshot, visible_snapshot, write_app, write_lib, write_workspace};
use beskid_tests_release::registry::Registry;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::{Duration, Instant};

const APP: &str = "App { name = \"App\" version = \"0.1.0\" }\ntarget \"App\" { kind = \"App\" entry = \"Main.bd\" }\n";
const CRLF_MANIFEST: &str = "// caf\u{e9}: keep this comment\r\nApp {\r\n  name = \"App\"\r\n  version = \"0.1.0\"\r\n}\r\n\r\ntarget \"App\" {\r\n  kind = \"App\"\r\n  entry = \"Main.bd\"\r\n}\r\n";
const ONE_STABLE: &str = r#"[{"version":"1.0.0","isYanked":false}]"#;

fn run(cli: &Cli, home: &Home, args: &[&str]) -> Output {
    cli.run(cli.command(home).args(args))
}
fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
fn assert_success(output: &Output, what: &str) {
    assert!(output.status.success(), "{what} failed:\n{}", transcript(output));
}
fn path_text(path: &Path) -> &str {
    path.to_str().expect("UTF-8 path")
}

/// A registry-backed application in a workspace whose `default` registry is the loopback server.
struct Fixture {
    manifest: PathBuf,
}
impl Fixture {
    fn new(home: &Home, registry: &Registry) -> Self {
        let root = home.work();
        std::fs::create_dir_all(root.join("App/Src")).unwrap();
        std::fs::write(root.join("App/Src/Main.bd"), "i32 Main() { return 0; }\n").unwrap();
        write_workspace(&root, &["App"], "default", registry.url());
        let manifest = root.join("App/App.bproj");
        std::fs::write(&manifest, APP).unwrap();
        Self { manifest }
    }
    fn project(&self) -> &Path {
        self.manifest.parent().unwrap()
    }
    fn manifest_text(&self) -> &str {
        path_text(&self.manifest)
    }
    fn lock_path(&self) -> PathBuf {
        self.manifest.with_file_name("Project.lock")
    }
    fn lock(&self) -> String {
        std::fs::read_to_string(self.lock_path()).expect("Project.lock")
    }
    fn manifest(&self) -> String {
        std::fs::read_to_string(&self.manifest).unwrap()
    }
    /// Visible project bytes; private `.beskid` transaction state is excluded.
    fn snapshot(&self) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
        visible_snapshot(self.project())
    }
}

// ---------------------------------------------------------------------------------------------
// DEP06-01 Text preservation, repetition, unsupported sources, workspace targeting

#[test]
fn dep06_01_add_preserves_crlf_unicode_comments_outside_edited_span() {
    let cli = Cli::qualified();
    let home = Home::new();
    write_lib(&home.work(), "Local", "pub i32 Value() { return 1; }\n");
    let manifest = write_app(&home.work(), "App", "i32 Main() { return 0; }\n");
    std::fs::write(&manifest, CRLF_MANIFEST).unwrap();
    assert_success(&run(cli, &home, &["add", "Local", "--path", "../Local", "--offline", "--project", path_text(&manifest)]), "add");
    let updated = std::fs::read_to_string(&manifest).unwrap();
    assert!(updated.starts_with(CRLF_MANIFEST), "bytes before the edited span are preserved byte for byte:\n{updated:?}");
    assert!(!updated.replace("\r\n", "").contains('\n'), "the manifest keeps CRLF line endings throughout:\n{updated:?}");
    assert!(updated.contains("// caf\u{e9}: keep this comment"), "comments and Unicode survive");
    assert!(updated.contains("dependency \"Local\"") && updated.contains("path = \"../Local\""), "the intent is declared:\n{updated}");
}

#[test]
fn dep06_01_repeated_add_is_noop_and_conflicting_source_fails_without_rewrite() {
    let cli = Cli::qualified();
    let home = Home::new();
    let registry = Registry::new();
    write_lib(&home.work(), "Local", "pub i32 Value() { return 1; }\n");
    let fixture = Fixture::new(&home, &registry);
    assert_success(&run(cli, &home, &["add", "Local", "--path", "../Local", "--offline", "--project", fixture.manifest_text()]), "first add");
    let before = fixture.snapshot();
    assert_success(&run(cli, &home, &["add", "Local", "--path", "../Local", "--offline", "--project", fixture.manifest_text()]), "repeated add");
    assert_eq!(fixture.snapshot(), before, "repeated identical intent preserves every project byte");
    let conflict = run(cli, &home, &["add", "Local@1.0.0", "--offline", "--project", fixture.manifest_text()]);
    assert!(!conflict.status.success(), "a conflicting source for an existing dependency fails:\n{}", transcript(&conflict));
    assert_eq!(fixture.snapshot(), before, "a rejected conflict rewrites nothing");
    assert_eq!(registry.count(), 0);
}

#[test]
fn dep06_01_git_and_range_selectors_return_usage_guidance() {
    let cli = Cli::qualified();
    let home = Home::new();
    let registry = Registry::new();
    let fixture = Fixture::new(&home, &registry);
    let before = fixture.snapshot();
    for args in [vec!["add", "Numbers@^1.0"], vec!["add", "Numbers@1.x"], vec!["add", "Numbers@>=1.0.0"], vec!["add", "Numbers", "--git", "https://example.invalid/numbers.git"]] {
        let mut full = args.clone();
        full.extend(["--project", fixture.manifest_text()]);
        let output = run(cli, &home, &full);
        assert_eq!(output.status.code(), Some(2), "{args:?} is a usage error:\n{}", transcript(&output));
        let diagnostic = text(&output.stderr).to_ascii_lowercase();
        assert!(diagnostic.contains("exact") || diagnostic.contains("unsupported") || diagnostic.contains("--help"), "{args:?} explains the exact-version grammar:\n{diagnostic}");
    }
    assert_eq!(fixture.snapshot(), before, "usage errors write nothing");
    assert_eq!(registry.count(), 0, "usage errors contact no registry");
}

#[test]
fn dep06_01_workspace_ambiguity_lists_sorted_candidates_and_writes_nothing() {
    let cli = Cli::qualified();
    let home = Home::new();
    let registry = Registry::new();
    for name in ["Zulu", "Alpha"] {
        write_app(&home.work(), name, "i32 Main() { return 0; }\n");
    }
    write_lib(&home.work(), "Local", "pub i32 Value() { return 1; }\n");
    write_workspace(&home.work(), &["Zulu", "Alpha"], "default", registry.url());
    let before = snapshot(&home.work());
    let output = run(cli, &home, &["add", "Local", "--path", "../Local", "--offline"]);
    assert!(!output.status.success(), "an ambiguous workspace needs --project:\n{}", transcript(&output));
    let diagnostic = text(&output.stderr);
    assert!(diagnostic.contains("--project"), "guidance names --project:\n{diagnostic}");
    let alpha = diagnostic.find("Alpha.bproj").expect("candidates are listed");
    let zulu = diagnostic.find("Zulu.bproj").expect("candidates are listed");
    assert!(alpha < zulu, "candidates are sorted, not in member order:\n{diagnostic}");
    assert_eq!(snapshot(&home.work()), before, "ambiguity writes nothing");
}

#[test]
fn dep06_01_explicit_member_with_spaced_relative_path_changes_only_that_member() {
    let cli = Cli::qualified();
    let home = Home::new();
    let registry = Registry::new();
    for name in ["Zulu", "Alpha"] {
        write_app(&home.work(), name, "i32 Main() { return 0; }\n");
    }
    let shared = home.work().join("with spaces");
    std::fs::create_dir_all(shared.join("Src")).unwrap();
    std::fs::write(shared.join("Src/Shared.bd"), "pub i32 Value() { return 1; }\n").unwrap();
    std::fs::write(shared.join("Shared.bproj"), "Shared { name = \"Shared\" version = \"0.1.0\" }\ntarget \"Shared\" { kind = \"Lib\" entry = \"Shared.bd\" }\n").unwrap();
    write_workspace(&home.work(), &["Zulu", "Alpha"], "default", registry.url());
    let alpha_before = snapshot(&home.work().join("Alpha"));
    let zulu_manifest = home.work().join("Zulu/Zulu.bproj");
    assert_success(&run(cli, &home, &["add", "Shared", "--path", "../with spaces", "--offline", "--project", path_text(&zulu_manifest)]), "add to the selected member");
    let zulu = std::fs::read_to_string(&zulu_manifest).unwrap();
    assert!(zulu.contains("path = \"../with spaces\""), "the relative path with spaces is kept verbatim:\n{zulu}");
    assert!(zulu_manifest.with_file_name("Project.lock").is_file());
    assert_eq!(snapshot(&home.work().join("Alpha")), alpha_before, "the unselected member is untouched");
}

#[test]
fn dep06_01_path_report_keeps_source_coordinates_without_manufacturing_a_version() {
    let cli = Cli::qualified();
    let home = Home::new();
    write_lib(&home.work(), "Local", "pub i32 Value() { return 1; }\n");
    let manifest = write_app(&home.work(), "App", "i32 Main() { return 0; }\n");
    let added = run(cli, &home, &["add", "Local", "--path", "../Local", "--offline", "--project", path_text(&manifest)]);
    assert_success(&added, "add");
    let report = text(&added.stdout);
    assert!(report.contains("added Local: path ../Local"), "the add report names the actual path coordinate:\n{report}");
    assert!(!report.contains("absent -> absent") && !report.contains("0.0.0"), "no version is manufactured for a path dependency:\n{report}");
    let removed = run(cli, &home, &["remove", "Local", "--offline", "--project", path_text(&manifest)]);
    assert_success(&removed, "remove");
    let report = text(&removed.stdout);
    assert!(report.contains("removed Local: path ../Local"), "the remove report names the actual path coordinate:\n{report}");
    assert!(!report.contains("absent -> absent"), "{report}");
}

// ---------------------------------------------------------------------------------------------
// DEP06-02 Strict registry coordinates and stable addition

#[test]
fn dep06_02_exact_request_fails_without_substitution_and_pair_unchanged() {
    let cli = Cli::qualified();
    let home = Home::new();
    let registry = Registry::new();
    registry.versions("Numbers", ONE_STABLE);
    registry.package("Numbers", "1.0.0", 1);
    let fixture = Fixture::new(&home, &registry);
    let before = fixture.snapshot();
    let output = run(cli, &home, &["add", "Numbers@1.2.0", "--project", fixture.manifest_text()]);
    assert_eq!(output.status.code(), Some(1), "an absent exact version fails without substitution:\n{}", transcript(&output));
    let diagnostic = text(&output.stderr);
    assert!(diagnostic.contains("Numbers") && diagnostic.contains("1.2.0"), "the diagnostic names package and requested version:\n{diagnostic}");
    assert!(!diagnostic.contains("1.0.0 instead") && !text(&output.stdout).contains("added"), "no other version is substituted:\n{}", transcript(&output));
    assert_eq!(fixture.snapshot(), before, "manifest and lock are unchanged");
    assert!(!fixture.lock_path().exists());
}

#[test]
fn dep06_02_malformed_response_and_503_fail_without_warning_success() {
    let cli = Cli::qualified();
    for (label, status, body) in [("malformed", 200u16, "not JSON"), ("unavailable", 503u16, "service unavailable")] {
        let registry = Registry::new();
        registry.route("/api/packages/Numbers/versions", status, body.as_bytes().to_vec());
        let scoped = Home::new();
        let fixture = Fixture::new(&scoped, &registry);
        let before = fixture.snapshot();
        let output = run(cli, &scoped, &["add", "Numbers@1.0.0", "--project", fixture.manifest_text()]);
        assert_eq!(output.status.code(), Some(1), "{label} registry response fails the add:\n{}", transcript(&output));
        let combined = format!("{}{}", text(&output.stdout), text(&output.stderr)).to_ascii_lowercase();
        assert!(!combined.contains("added numbers"), "{label}: no success is reported:\n{combined}");
        assert!(!(combined.contains("warning") && combined.contains("unresolved")), "{label}: no warning-only success:\n{combined}");
        assert!(combined.contains("numbers"), "{label}: the diagnostic names the package:\n{combined}");
        assert!(!combined.contains("token") && !combined.contains("api_key"), "{label}: credential-free guidance:\n{combined}");
        assert_eq!(fixture.snapshot(), before, "{label}: the pair is unchanged");
        assert!(registry.count() >= 1, "{label}: the registry was consulted");
    }
}

#[test]
fn dep06_02_bare_add_selects_greatest_stable_independent_of_response_order() {
    let cli = Cli::qualified();
    let listings = [
        r#"[{"version":"1.0.0","isYanked":false},{"version":"2.0.0","isYanked":false},{"version":"3.0.0-rc.1","isYanked":false}]"#,
        r#"[{"version":"3.0.0-rc.1","isYanked":false},{"version":"2.0.0","isYanked":false},{"version":"1.0.0","isYanked":false}]"#,
        r#"[{"version":"2.0.0","isYanked":false},{"version":"3.0.0-rc.1","isYanked":false},{"version":"1.0.0","isYanked":false},{"version":"4.0.0","isYanked":true}]"#,
    ];
    for listing in listings {
        let home = Home::new();
        let registry = Registry::new();
        registry.versions("Numbers", listing);
        registry.package("Numbers", "2.0.0", 2);
        let fixture = Fixture::new(&home, &registry);
        assert_success(&run(cli, &home, &["add", "Numbers", "--project", fixture.manifest_text()]), "bare add");
        assert!(fixture.manifest().contains("version = \"2.0.0\""), "exact stable 2.0.0 is recorded regardless of order:\n{}", fixture.manifest());
        assert!(lock_pin(&fixture.lock(), "Numbers").expect("Numbers pin").contains("resolved_version=2.0.0"), "{}", fixture.lock());
    }
}

// ---------------------------------------------------------------------------------------------
// DEP06-03 Explicit conservative update and removal

fn two_exact_dependencies(cli: &Cli, home: &Home, registry: &Registry) -> Fixture {
    for name in ["Numbers", "Other"] {
        registry.versions(name, ONE_STABLE);
        registry.package(name, "1.0.0", 1);
        registry.package(name, "2.0.0", 2);
    }
    let fixture = Fixture::new(home, registry);
    for name in ["Numbers", "Other"] {
        assert_success(&run(cli, home, &["add", &format!("{name}@1.0.0"), "--project", fixture.manifest_text()]), &format!("add {name}"));
    }
    for name in ["Numbers", "Other"] {
        registry.versions(name, r#"[{"version":"1.0.0","isYanked":false},{"version":"2.0.0","isYanked":false}]"#);
    }
    fixture
}

#[test]
fn dep06_03_update_without_scope_exits_2() {
    let cli = Cli::qualified();
    let home = Home::new();
    let registry = Registry::new();
    let fixture = Fixture::new(&home, &registry);
    let before = fixture.snapshot();
    for args in [vec!["update"], vec!["update", "Numbers", "--all"]] {
        let mut full = args.clone();
        full.extend(["--project", fixture.manifest_text()]);
        let output = run(cli, &home, &full);
        assert_eq!(output.status.code(), Some(2), "{args:?} is a usage error:\n{}", transcript(&output));
        let diagnostic = text(&output.stderr);
        assert!(diagnostic.contains("--all") || diagnostic.contains("package"), "{args:?} shows scope guidance:\n{diagnostic}");
    }
    assert_eq!(fixture.snapshot(), before);
    assert_eq!(registry.count(), 0);
}

#[test]
fn dep06_03_selected_and_all_refresh_preserve_exact_intent_and_unrelated_pins() {
    let cli = Cli::qualified();
    let home = Home::new();
    let registry = Registry::new();
    let fixture = two_exact_dependencies(cli, &home, &registry);
    let manifest_before = fixture.manifest();
    let other_pin = lock_pin(&fixture.lock(), "Other").expect("Other pin");
    let numbers_pin = lock_pin(&fixture.lock(), "Numbers").expect("Numbers pin");
    assert_success(&run(cli, &home, &["update", "Numbers", "--project", fixture.manifest_text()]), "selected update without version");
    assert_eq!(fixture.manifest(), manifest_before, "selected refresh never rewrites exact intent");
    assert_eq!(lock_pin(&fixture.lock(), "Other").unwrap(), other_pin, "the unselected pin is preserved");
    assert!(lock_pin(&fixture.lock(), "Numbers").unwrap().contains("resolved_version=1.0.0"), "exact 1.0.0 stays pinned:\n{}", fixture.lock());
    assert_success(&run(cli, &home, &["update", "--all", "--project", fixture.manifest_text()]), "update --all");
    assert_eq!(fixture.manifest(), manifest_before, "--all never rewrites exact intent");
    assert_eq!(lock_pin(&fixture.lock(), "Other").unwrap(), other_pin);
    assert_eq!(lock_pin(&fixture.lock(), "Numbers").unwrap(), numbers_pin);
}

#[test]
fn dep06_03_update_version_changes_only_selected_intent_and_reachable_closure() {
    let cli = Cli::qualified();
    let home = Home::new();
    let registry = Registry::new();
    let fixture = two_exact_dependencies(cli, &home, &registry);
    let other_pin = lock_pin(&fixture.lock(), "Other").expect("Other pin");
    assert_success(&run(cli, &home, &["update", "Numbers", "--version", "2.0.0", "--project", fixture.manifest_text()]), "update --version");
    let manifest = fixture.manifest();
    assert!(manifest.contains("dependency \"Numbers\"") && manifest.matches("version = \"2.0.0\"").count() == 1, "only the selected intent moves to 2.0.0:\n{manifest}");
    assert!(manifest.matches("version = \"1.0.0\"").count() == 1, "the other exact intent stays 1.0.0:\n{manifest}");
    assert!(lock_pin(&fixture.lock(), "Numbers").unwrap().contains("resolved_version=2.0.0"), "{}", fixture.lock());
    assert_eq!(lock_pin(&fixture.lock(), "Other").unwrap(), other_pin, "the unrelated pin is byte-identical");
}

#[test]
fn dep06_03_removing_one_direct_keeps_shared_transitive_locked() {
    let cli = Cli::qualified();
    let home = Home::new();
    let registry = Registry::new();
    registry.versions("Shared", ONE_STABLE);
    registry.package("Shared", "1.0.0", 1);
    for name in ["Alpha", "Beta"] {
        registry.versions(name, ONE_STABLE);
        registry.package_depending_on(name, "1.0.0", ("Shared", "1.0.0"), "pub i32 Value() { return 1; }\n");
    }
    let fixture = Fixture::new(&home, &registry);
    for name in ["Alpha", "Beta"] {
        assert_success(&run(cli, &home, &["add", &format!("{name}@1.0.0"), "--project", fixture.manifest_text()]), &format!("add {name}"));
    }
    let shared_pin = lock_pin(&fixture.lock(), "Shared").expect("the shared transitive is locked");
    assert_success(&run(cli, &home, &["remove", "Alpha", "--project", fixture.manifest_text()]), "remove Alpha");
    let lock = fixture.lock();
    assert!(lock_pin(&lock, "Alpha").is_none(), "the removed direct is gone:\n{lock}");
    assert!(lock_pin(&lock, "Beta").is_some(), "the remaining direct stays:\n{lock}");
    assert_eq!(lock_pin(&lock, "Shared").as_deref(), Some(shared_pin.as_str()), "the transitive reachable from Beta stays locked:\n{lock}");
}

#[test]
fn dep06_03_update_dry_run_reports_effects_and_changes_no_bytes() {
    let cli = Cli::qualified();
    let home = Home::new();
    let registry = Registry::new();
    let fixture = two_exact_dependencies(cli, &home, &registry);
    let before = fixture.snapshot();
    let output = run(cli, &home, &["update", "Numbers", "--version", "2.0.0", "--dry-run", "--project", fixture.manifest_text()]);
    assert_success(&output, "update --dry-run");
    let report = text(&output.stdout);
    assert!(report.contains("Numbers") && report.contains("1.0.0") && report.contains("2.0.0"), "the dry run reports old and new coordinates:\n{report}");
    assert_eq!(fixture.snapshot(), before, "a dry run changes no manifest, lock or materialized byte");
}

// ---------------------------------------------------------------------------------------------
// DEP06-04 Independent lock and network policies

fn locked_registry_app(cli: &Cli, home: &Home, registry: &Registry) -> Fixture {
    registry.versions("Numbers", ONE_STABLE);
    registry.package("Numbers", "1.0.0", 1);
    let fixture = Fixture::new(home, registry);
    assert_success(&run(cli, home, &["add", "Numbers@1.0.0", "--project", fixture.manifest_text()]), "add Numbers");
    fixture
}

fn remove_materialized_cache(fixture: &Fixture) {
    for directory in ["obj", ".beskid"] {
        let _ = std::fs::remove_dir_all(fixture.project().join(directory));
    }
}

#[test]
fn dep06_04_build_locked_fetches_exact_pin_without_lock_write() {
    let cli = Cli::qualified();
    let home = Home::new();
    let registry = Registry::new();
    let fixture = locked_registry_app(cli, &home, &registry);
    let lock_before = fixture.lock();
    remove_materialized_cache(&fixture);
    let requests_before = registry.count();
    let output = run(cli, &home, &["build", "--locked", "--plain", "--project", fixture.manifest_text()]);
    assert_success(&output, "build --locked with a cold cache");
    assert!(registry.requests()[requests_before..].iter().any(|path| path.ends_with("/1.0.0/download")), "the exact pin is fetched:\n{:?}", registry.requests());
    assert_eq!(fixture.lock(), lock_before, "--locked never writes the lock");
}

#[test]
fn dep06_04_build_offline_warm_succeeds_cold_and_tampered_fail_with_zero_requests() {
    let cli = Cli::qualified();
    let home = Home::new();
    let registry = Registry::new();
    let fixture = locked_registry_app(cli, &home, &registry);
    assert_success(&run(cli, &home, &["build", "--plain", "--project", fixture.manifest_text()]), "warm the cache");
    let count = registry.count();
    assert_success(&run(cli, &home, &["build", "--offline", "--plain", "--project", fixture.manifest_text()]), "warm offline build");
    assert_eq!(registry.count(), count, "warm offline makes no request");
    let cached = snapshot(fixture.project()).into_keys().find(|path| path.starts_with("obj") && path.ends_with("Main.bd") && path.to_string_lossy().contains("Numbers")).expect("materialized registry source");
    std::fs::write(fixture.project().join(&cached), "tampered\n").unwrap();
    let tampered = run(cli, &home, &["build", "--offline", "--plain", "--project", fixture.manifest_text()]);
    assert_eq!(tampered.status.code(), Some(1), "tampered cache fails offline:\n{}", transcript(&tampered));
    assert_eq!(registry.count(), count, "tampered offline makes no request");
    remove_materialized_cache(&fixture);
    let cold = run(cli, &home, &["build", "--offline", "--plain", "--project", fixture.manifest_text()]);
    assert_eq!(cold.status.code(), Some(1), "cold cache fails offline:\n{}", transcript(&cold));
    let diagnostic = text(&cold.stderr).to_ascii_lowercase();
    assert!(diagnostic.contains("offline") && (diagnostic.contains("cache") || diagnostic.contains("pin")), "cold failure names the offline cache condition:\n{diagnostic}");
    assert_eq!(registry.count(), count, "cold offline makes no request");
    assert!(!fixture.project().join("obj").exists(), "cold offline fails before executable preparation");
}

#[test]
fn dep06_04_build_frozen_rejects_missing_or_stale_lock_without_requests() {
    let cli = Cli::qualified();
    let home = Home::new();
    let registry = Registry::new();
    let fixture = locked_registry_app(cli, &home, &registry);
    let lock = fixture.lock();
    let count = registry.count();
    std::fs::remove_file(fixture.lock_path()).unwrap();
    let missing = run(cli, &home, &["build", "--frozen", "--plain", "--project", fixture.manifest_text()]);
    assert_eq!(missing.status.code(), Some(1), "frozen rejects a missing lock:\n{}", transcript(&missing));
    assert!(!fixture.lock_path().exists(), "frozen never writes a lock");
    let stale = lock.replace("resolved_version=1.0.0", "resolved_version=0.9.0");
    assert_ne!(stale, lock);
    std::fs::write(fixture.lock_path(), &stale).unwrap();
    let drifted = run(cli, &home, &["build", "--frozen", "--plain", "--project", fixture.manifest_text()]);
    assert_eq!(drifted.status.code(), Some(1), "frozen rejects a stale lock:\n{}", transcript(&drifted));
    assert_eq!(fixture.lock(), stale, "frozen never rewrites the lock");
    assert_eq!(registry.count(), count, "frozen makes no request");
}

#[test]
fn dep06_04_doc_offline_fails_before_network_or_output() {
    let cli = Cli::qualified();
    let home = Home::new();
    let registry = Registry::new();
    let fixture = Fixture::new(&home, &registry);
    std::fs::write(&fixture.manifest, format!("{APP}dependency \"Numbers\" {{ source = \"registry\" version = \"1.0.0\" }}\n")).unwrap();
    let before = fixture.snapshot();
    let output = run(cli, &home, &["doc", "--offline", "--project", fixture.manifest_text()]);
    assert_eq!(output.status.code(), Some(1), "offline doc with an uncached dependency fails:\n{}", transcript(&output));
    let diagnostic = text(&output.stderr).to_ascii_lowercase();
    assert!(diagnostic.contains("offline"), "{diagnostic}");
    assert_eq!(registry.count(), 0, "doc shares the offline policy: no request");
    assert_eq!(fixture.snapshot(), before, "no output or lock is written");
    assert!(!home.work().join("doc-out").exists() && !home.work().join("App/doc").exists());
}

// ---------------------------------------------------------------------------------------------
// DEP06-05 Transactional manifest and lock replacement

#[test]
fn dep06_05_registry_failure_during_add_leaves_original_pair() {
    let cli = Cli::qualified();
    let home = Home::new();
    let registry = Registry::new();
    registry.versions("Numbers", ONE_STABLE);
    registry.package("Numbers", "1.0.0", 1);
    // The version listing is served; the first download and everything after answers 503.
    registry.fail_after(1);
    let fixture = Fixture::new(&home, &registry);
    let before = fixture.snapshot();
    let output = run(cli, &home, &["add", "Numbers@1.0.0", "--project", fixture.manifest_text()]);
    assert_eq!(output.status.code(), Some(1), "a download failure during add exits nonzero:\n{}", transcript(&output));
    assert_eq!(fixture.snapshot(), before, "the original pair is unchanged");
    assert!(!fixture.lock_path().exists(), "the missing-lock state is preserved");
    assert!(registry.requests().iter().any(|path| path.ends_with("/download")), "the failure happened at download: {:?}", registry.requests());
}

/// Transaction I/O that exits the process after the first visible project replacement (the manifest),
/// as process death would. Private journal publications under `.beskid` complete normally first.
struct CrashIo;
impl beskid_analysis::projects::dependency_transaction::DependencyTransactionIo for CrashIo {
    fn Replace(&self, source: &Path, destination: &Path) -> std::io::Result<()> {
        beskid_analysis::projects::dependency_transaction::NativeDependencyTransactionIo.Replace(source, destination)?;
        if destination.components().any(|component| component.as_os_str() == ".beskid") {
            return Ok(());
        }
        std::process::exit(91);
    }
    fn SyncFile(&self, file: &std::fs::File) -> std::io::Result<()> {
        beskid_analysis::projects::dependency_transaction::NativeDependencyTransactionIo.SyncFile(file)
    }
    fn SyncDirectory(&self, path: &Path) -> std::io::Result<()> {
        beskid_analysis::projects::dependency_transaction::NativeDependencyTransactionIo.SyncDirectory(path)
    }
}

const CRASH_ENV: &str = "BESKID_RELEASE_DEPENDENCY_CRASH_MANIFEST";

#[test]
fn dep06_05_interrupted_pair_is_recovered_by_next_installed_command_before_reads() {
    use beskid_analysis::projects::dependency_transaction::CommitDependencyPairWithIo;
    if let Some(path) = std::env::var_os(CRASH_ENV) {
        // Child role: die after the first replacement of the pinned transaction, leaving its journal.
        let manifest = PathBuf::from(path);
        let original = std::fs::read(&manifest).unwrap();
        let original_lock = std::fs::read(manifest.with_file_name("Project.lock")).unwrap();
        let replacement = format!("{}// interrupted replacement\n", String::from_utf8_lossy(&original));
        CommitDependencyPairWithIo(&manifest, original.as_slice(), Some(original_lock.as_slice()), replacement.as_bytes(), b"lock-replacement\n", &CrashIo).unwrap();
        panic!("crash seam did not exit");
    }
    let cli = Cli::qualified();
    let home = Home::new();
    write_lib(&home.work(), "Local", "pub i32 Value() { return 1; }\n");
    let manifest = write_app(&home.work(), "App", "i32 Main() { return 0; }\n");
    assert_success(&run(cli, &home, &["add", "Local", "--path", "../Local", "--offline", "--project", path_text(&manifest)]), "add path dependency");
    let original = std::fs::read(&manifest).unwrap();
    let original_lock = std::fs::read(manifest.with_file_name("Project.lock")).unwrap();
    // The child's libtest streams are captured, never inherited: an inherited run header would land
    // in this harness's stdout and break the single-run libtest output the release case requires.
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "dependency_journeys_v06::dep06_05_interrupted_pair_is_recovered_by_next_installed_command_before_reads", "--nocapture"])
        .env(CRASH_ENV, &manifest)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert_eq!(child.status.code(), Some(91), "the child died after the first replacement:\n{}", transcript(&child));
    assert_ne!(std::fs::read(&manifest).unwrap(), original, "the manifest shows the interrupted replacement");
    assert!(manifest.parent().unwrap().join(".beskid").exists(), "the private journal survives the death");
    let check = run(cli, &home, &["check", "--plain", "--offline", "--project", path_text(&manifest)]);
    assert_success(&check, "the next installed command");
    assert_eq!(std::fs::read(&manifest).unwrap(), original, "the original manifest is recovered before reads");
    assert_eq!(std::fs::read(manifest.with_file_name("Project.lock")).unwrap(), original_lock, "the original lock is recovered");
}

#[test]
fn dep06_05_concurrent_manifest_edit_fails_commit_without_overwrite() {
    let cli = Cli::qualified();
    let home = Home::new();
    let registry = Registry::new();
    registry.versions("Numbers", ONE_STABLE);
    registry.package("Numbers", "1.0.0", 1);
    registry.hold("/api/packages/Numbers/versions/1.0.0/download");
    let fixture = Fixture::new(&home, &registry);
    let child = cli.spawn(cli.command(&home).args(["add", "Numbers@1.0.0", "--project", fixture.manifest_text()]));
    let started = Instant::now();
    while !registry.held() {
        assert!(started.elapsed() < Duration::from_secs(120), "the add never reached the held download");
        std::thread::sleep(Duration::from_millis(10));
    }
    let external = format!("{APP}// concurrent user edit\n");
    std::fs::write(&fixture.manifest, &external).unwrap();
    registry.release();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(1), "a commit planned against stale bytes fails:\n{}", transcript(&output));
    assert_eq!(fixture.manifest(), external, "the external edit is preserved, never overwritten");
    assert!(!fixture.lock_path().exists(), "no lock is written for a failed commit");
}
