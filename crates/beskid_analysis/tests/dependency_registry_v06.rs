use beskid_analysis::projects::dependency_edit::{
    CommitDependencyChange, DependencyIntent, DependencyIntentSource, DependencyMutation, PlanDependencyChange,
};
use beskid_analysis::projects::workflow::{RefreshScope, ResolutionPolicy};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

type Routes = BTreeMap<String, (u16, Vec<u8>)>;
struct Registry {
    url: String,
    routes: Arc<Mutex<Routes>>,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Registry {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let routes = Arc::new(Mutex::new(Routes::new()));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (r, q, s) = (routes.clone(), requests.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            while !s.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                        let mut bytes = Vec::new();
                        let mut chunk = [0; 1024];
                        while !bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                            let count = stream.read(&mut chunk).unwrap_or(0);
                            if count == 0 {
                                break;
                            }
                            bytes.extend_from_slice(&chunk[..count]);
                            assert!(bytes.len() < 16384);
                        }
                        let text = String::from_utf8(bytes).unwrap();
                        let path = text.split_whitespace().nth(1).unwrap_or("").to_owned();
                        q.lock().unwrap().push(path.clone());
                        let (status, body) =
                            r.lock().unwrap().get(&path).cloned().unwrap_or((404, b"missing".to_vec()));
                        write!(
                            stream,
                            "HTTP/1.1 {status} Response\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .unwrap();
                        stream.write_all(&body).unwrap();
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2))
                    }
                    Err(error) => panic!("loopback accept: {error}"),
                }
            }
        });
        Self { url, routes, requests, stop, thread: Some(thread) }
    }
    fn versions(&self, name: &str, body: &str) {
        self.routes.lock().unwrap().insert(format!("/api/packages/{name}/versions"), (200, body.as_bytes().to_vec()));
    }
    fn package(&self, name: &str, version: &str) {
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        archive.start_file(format!("{name}.bproj"), options).unwrap();
        write!(archive, "{name} {{ name = \"{name}\" version = \"{version}\" }}\ntarget \"{name}\" {{ kind = \"Lib\" entry = \"Main.bd\" }}\n").unwrap();
        archive.start_file("src/Main.bd", options).unwrap();
        archive.write_all(b"i32 Value() { return 1; }\n").unwrap();
        let bytes = archive.finish().unwrap().into_inner();
        self.routes.lock().unwrap().insert(format!("/api/packages/{name}/versions/{version}/download"), (200, bytes));
    }
    fn count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}
impl Drop for Registry {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.thread.take().unwrap().join().unwrap();
    }
}

struct Fixture {
    _root: tempfile::TempDir,
    manifest: PathBuf,
}
impl Fixture {
    fn new(registry: &Registry) -> Self {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("App")).unwrap();
        std::fs::write(root.path().join("Workspace.bws"), format!("workspace {{ name = \"Test\" resolver = v1 }}\nmember \"App\" {{ path = \"App\" }}\nregistry \"test\" {{ url = \"{}\" }}\n", registry.url)).unwrap();
        let manifest = root.path().join("App/App.bproj");
        std::fs::write(
            &manifest,
            "App { name = \"App\" version = \"0.1.0\" }\ntarget \"App\" { kind = \"App\" entry = \"Main.bd\" }\n",
        )
        .unwrap();
        Self { _root: root, manifest }
    }
    fn snapshot(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        snapshot(self.manifest.parent().unwrap())
    }
    fn commit(&self, mutation: DependencyMutation, policy: ResolutionPolicy) {
        let plan = PlanDependencyChange(&self.manifest, &mutation, &policy).unwrap();
        CommitDependencyChange(plan).unwrap();
    }
    fn lock(&self) -> String {
        std::fs::read_to_string(self.manifest.with_file_name("Project.lock")).unwrap()
    }
}
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, at: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in std::fs::read_dir(at).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                out.insert(path.strip_prefix(root).unwrap().to_owned(), std::fs::read(path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}
fn policy(offline: bool, refresh: RefreshScope) -> ResolutionPolicy {
    ResolutionPolicy { locked: false, offline, refresh }
}
fn add(name: &str, version: &str) -> DependencyMutation {
    DependencyMutation::Add(DependencyIntent {
        name: name.into(),
        source: DependencyIntentSource::Registry { registry: Some("test".into()), version: version.into() },
    })
}
fn update(name: &str, version: Option<&str>) -> DependencyMutation {
    DependencyMutation::Update { package: Some(name.into()), version: version.map(str::to_owned), all: false }
}
fn selected(name: &str) -> RefreshScope {
    RefreshScope::Selected(BTreeSet::from([name.into()]))
}
fn pin(lock: &str, name: &str) -> String {
    lock.lines().find(|line| line.starts_with(&format!("- name={name};"))).expect("dependency pin").to_owned()
}

#[test]
fn dep06_registry_bare_add_records_highest_stable_exact_intent_and_repeated_add_is_noop() {
    let registry = Registry::new();
    registry.versions("Numbers", r#"[{"version":"2.0.0-beta.1","isYanked":false},{"version":"1.2.0","isYanked":false},{"version":"3.0.0","isYanked":true},{"version":"1.10.0","isYanked":false}]"#);
    registry.package("Numbers", "1.10.0");
    let fixture = Fixture::new(&registry);
    fixture.commit(add("Numbers", ""), policy(false, RefreshScope::None));
    let parsed =
        beskid_analysis::projects::parse_manifest(&std::fs::read_to_string(&fixture.manifest).unwrap()).unwrap();
    assert_eq!(parsed.dependencies[0].version.as_deref(), Some("1.10.0"));
    assert!(pin(&fixture.lock(), "Numbers").contains("resolved_version=1.10.0"));
    let before = fixture.snapshot();
    fixture.commit(add("Numbers", "1.10.0"), policy(false, RefreshScope::None));
    assert_eq!(fixture.snapshot(), before, "repeated exact intent must preserve all project bytes");
}

#[test]
fn dep06_registry_selected_refresh_preserves_unselected_pin() {
    let registry = Registry::new();
    for name in ["Numbers", "Other"] {
        registry.versions(name, r#"[{"version":"1.0.0","isYanked":false}]"#);
        registry.package(name, "1.0.0");
        registry.package(name, "2.0.0");
    }
    let fixture = Fixture::new(&registry);
    fixture.commit(add("Numbers", "1.0.0"), policy(false, RefreshScope::None));
    fixture.commit(add("Other", "1.0.0"), policy(false, RefreshScope::None));
    let other = pin(&fixture.lock(), "Other");
    registry.versions("Numbers", r#"[{"version":"1.0.0","isYanked":false},{"version":"2.0.0","isYanked":false}]"#);
    registry.versions("Other", r#"[{"version":"1.0.0","isYanked":true},{"version":"2.0.0","isYanked":false}]"#);
    fixture.commit(update("Numbers", Some("2.0.0")), policy(false, selected("Numbers")));
    assert_eq!(pin(&fixture.lock(), "Other"), other);
    assert!(pin(&fixture.lock(), "Numbers").contains("resolved_version=2.0.0"));
}

#[test]
fn dep06_registry_offline_warm_cache_is_verified_without_http_and_tampering_fails_read_only() {
    let registry = Registry::new();
    registry.versions("Numbers", r#"[{"version":"1.0.0","isYanked":false}]"#);
    registry.package("Numbers", "1.0.0");
    let fixture = Fixture::new(&registry);
    fixture.commit(add("Numbers", "1.0.0"), policy(false, RefreshScope::None));
    let count = registry.count();
    let before = fixture.snapshot();
    fixture.commit(update("Numbers", None), policy(true, selected("Numbers")));
    assert_eq!(registry.count(), count, "offline warm resolution must issue no HTTP request");
    assert_eq!(fixture.snapshot(), before);
    let cache_file = before
        .keys()
        .find(|path| path.starts_with("obj") && path.ends_with("Main.bd"))
        .expect("materialized registry source");
    std::fs::write(fixture.manifest.parent().unwrap().join(cache_file), "tampered\n").unwrap();
    let tampered = fixture.snapshot();
    assert!(
        PlanDependencyChange(&fixture.manifest, &update("Numbers", None), &policy(true, selected("Numbers"))).is_err()
    );
    assert_eq!(registry.count(), count);
    assert_eq!(fixture.snapshot(), tampered, "failed cache verification must preserve manifest, lock and obj");
}

#[test]
fn dep06_registry_offline_cold_cache_fails_without_http_or_project_writes() {
    let registry = Registry::new();
    let fixture = Fixture::new(&registry);
    let before = fixture.snapshot();
    assert!(
        PlanDependencyChange(&fixture.manifest, &add("Numbers", "1.0.0"), &policy(true, RefreshScope::None)).is_err()
    );
    assert_eq!(registry.count(), 0);
    assert_eq!(fixture.snapshot(), before);
    assert!(!fixture.manifest.parent().unwrap().join("obj").exists());
    assert!(!fixture.manifest.with_file_name("Project.lock").exists());
}

#[test]
fn dep06_registry_unavailable_malformed_and_yanked_selection_fail_before_project_writes() {
    for body in [
        None,
        Some("not JSON"),
        Some(r#"[{"version":"1.0.0","isYanked":true}]"#),
        Some(r#"[{"version":"bogus","isYanked":false}]"#),
    ] {
        let registry = Registry::new();
        if let Some(body) = body {
            registry.versions("Numbers", body);
        }
        let fixture = Fixture::new(&registry);
        let before = fixture.snapshot();
        assert!(
            PlanDependencyChange(&fixture.manifest, &add("Numbers", ""), &policy(false, RefreshScope::None)).is_err(),
            "selection must fail for {body:?}"
        );
        assert_eq!(fixture.snapshot(), before);
        assert!(!fixture.manifest.parent().unwrap().join("obj").exists());
        assert!(!fixture.manifest.with_file_name("Project.lock").exists());
    }
}

fn ordinary_workspace_plan(fixture: &Fixture) -> beskid_analysis::projects::ProjectWorkspacePlan {
    use beskid_analysis::projects::{
        DependencySource, UnresolvedDependencyKind, UnresolvedDependencyNote, build_project_graph,
        collect_dependency_projects, collect_unresolved_dependencies,
    };
    let graph = build_project_graph(&fixture.manifest).unwrap();
    beskid_analysis::projects::ProjectWorkspacePlan {
        project_root: graph.root_project_root.clone(),
        manifest_path: graph.root_manifest_path.clone(),
        project_name: graph.root_manifest.project.name.clone(),
        source_root: Some(graph.root_project_root.join(&graph.root_manifest.project.root)),
        dependency_projects: collect_dependency_projects(&graph),
        unresolved_dependencies: collect_unresolved_dependencies(&graph)
            .into_iter()
            .map(|dependency| UnresolvedDependencyNote {
                dependency_name: dependency.dependency_name,
                source: match dependency.kind {
                    UnresolvedDependencyKind::Registry => DependencySource::Registry,
                    UnresolvedDependencyKind::Git => DependencySource::Git,
                },
                descriptor: dependency.descriptor,
            })
            .collect(),
    }
}

#[test]
fn dep06_ordinary_prepare_offline_cold_cache_is_read_only_and_never_contacts_registry() {
    use beskid_analysis::projects::{WorkspacePrepareOptions, prepare_project_workspace_plan_with_options};
    let registry = Registry::new();
    registry.versions("Numbers", r#"[{"version":"1.0.0","isYanked":false}]"#);
    registry.package("Numbers", "1.0.0");
    let fixture = Fixture::new(&registry);
    let original = std::fs::read_to_string(&fixture.manifest).unwrap();
    let proposed =
        beskid_analysis::projects::dependency_edit::EditManifest(&original, &add("Numbers", "1.0.0")).unwrap();
    std::fs::write(&fixture.manifest, proposed).unwrap();
    let plan = ordinary_workspace_plan(&fixture);
    let before = fixture.snapshot();
    let result = prepare_project_workspace_plan_with_options(
        &plan,
        WorkspacePrepareOptions { offline: true, ..Default::default() },
        None,
    );
    assert!(result.is_err(), "cold ordinary offline preparation must fail without fetching");
    assert_eq!(registry.count(), 0, "ordinary offline mapping must prohibit all HTTP");
    assert_eq!(fixture.snapshot(), before, "cold offline failure must preserve manifest, lock and project files");
    assert!(!fixture.manifest.parent().unwrap().join("obj").exists());
    assert!(!fixture.manifest.with_file_name("Project.lock").exists());
}

#[test]
fn dep06_ordinary_prepare_offline_requires_the_exact_lock_even_with_a_warm_verified_cache() {
    use beskid_analysis::projects::{WorkspacePrepareOptions, prepare_project_workspace_plan_with_options};
    let registry = Registry::new();
    registry.versions("Numbers", r#"[{"version":"1.0.0","isYanked":false}]"#);
    registry.package("Numbers", "1.0.0");
    let fixture = Fixture::new(&registry);
    fixture.commit(add("Numbers", "1.0.0"), policy(false, RefreshScope::None));
    let plan = ordinary_workspace_plan(&fixture);
    let source_root = plan.source_root.as_ref().unwrap();
    std::fs::create_dir_all(source_root).unwrap();
    std::fs::write(source_root.join("Main.bd"), "i32 Main() { return 0; }\n").unwrap();
    let lock_path = fixture.manifest.with_file_name("Project.lock");
    let expected_lock = fixture.lock();
    std::fs::remove_file(&lock_path).unwrap();
    let original_manifest = std::fs::read(&fixture.manifest).unwrap();
    let count = registry.count();
    // Offline never discovers a coordinate: without a lock pin there is no artifact digest to
    // verify, so a warm content-addressed cache cannot recreate the lock (DEP06-04 needs a lock).
    let error = prepare_project_workspace_plan_with_options(
        &plan,
        WorkspacePrepareOptions { offline: true, ..Default::default() },
        None,
    )
    .expect_err("offline preparation without a lock pin must fail closed");
    assert!(error.to_string().contains("requires an eligible exact lock pin"), "{error}");
    assert_eq!(registry.count(), count, "offline preparation must not perform HTTP");
    assert!(!lock_path.exists(), "failed offline preparation must not write a lock");
    assert_eq!(std::fs::read(&fixture.manifest).unwrap(), original_manifest);

    std::fs::write(&lock_path, &expected_lock).unwrap();
    prepare_project_workspace_plan_with_options(
        &plan,
        WorkspacePrepareOptions { offline: true, ..Default::default() },
        None,
    )
    .expect("warm ordinary offline lock replay succeeds");
    assert_eq!(registry.count(), count);
    assert_eq!(pin(&fixture.lock(), "Numbers"), pin(&expected_lock, "Numbers"));
    assert_eq!(fixture.lock(), expected_lock, "offline replay keeps the exact lock stable");
}

#[test]
fn dep06_ordinary_prepare_offline_locked_rejects_missing_lock_even_with_verified_cache() {
    use beskid_analysis::projects::{WorkspacePrepareOptions, prepare_project_workspace_plan_with_options};
    let registry = Registry::new();
    registry.versions("Numbers", r#"[{"version":"1.0.0","isYanked":false}]"#);
    registry.package("Numbers", "1.0.0");
    let fixture = Fixture::new(&registry);
    fixture.commit(add("Numbers", "1.0.0"), policy(false, RefreshScope::None));
    std::fs::remove_file(fixture.manifest.with_file_name("Project.lock")).unwrap();
    let plan = ordinary_workspace_plan(&fixture);
    let before = fixture.snapshot();
    let count = registry.count();
    let result = prepare_project_workspace_plan_with_options(
        &plan,
        WorkspacePrepareOptions { offline: true, locked: true, ..Default::default() },
        None,
    );
    assert!(
        matches!(result, Err(beskid_analysis::projects::ProjectError::LockfileRequired { .. })),
        "locked policy requires the existing lock independently from cache"
    );
    assert_eq!(registry.count(), count);
    assert_eq!(fixture.snapshot(), before);
    assert!(!fixture.manifest.with_file_name("Project.lock").exists());
}
