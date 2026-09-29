//! End-to-end registry pins against a loopback package server.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU8, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use beskid_analysis::projects::{
    PROJECT_LOCK_FILE_NAME, ProjectError, UnresolvedDependencyPolicy, WorkspacePrepareOptions,
    build_compile_plan_with_policy, prepare_project_workspace_with_options,
};
use tempfile::TempDir;

use super::with_cwd_at_workspace_root;

const OLD_VERSION: &str = "1.0.0";
const NEW_VERSION: &str = "2.0.0";
const OVER_LIMIT_BYTES: usize = 64 * 1024 * 1024 + 1;
const PACKAGE_MANIFEST: &[u8] = b"PkgCore {\n  name = \"PkgCore\"\n  version = \"0.1.0\"\n}\n\ntarget \"PkgCore\" {\n  kind = \"Lib\"\n  entry = \"Marker.bd\"\n}\n";

struct RegistryFixture {
    root: TempDir,
    app_manifest: PathBuf,
    packages: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    requests: Arc<Mutex<Vec<String>>>,
    oversized_download: Arc<AtomicU8>,
    stop: Arc<AtomicBool>,
    server: Option<JoinHandle<()>>,
}

impl RegistryFixture {
    fn new() -> Self {
        assert!(
            std::env::var_os("BESKID_PCKG_URL").is_none(),
            "unset BESKID_PCKG_URL so registry fixture requests stay on loopback"
        );
        let root = tempfile::tempdir().expect("temporary registry project");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local registry");
        listener.set_nonblocking(true).expect("set local listener nonblocking");
        let url = format!("http://{}", listener.local_addr().expect("registry address"));
        let packages = Arc::new(Mutex::new(BTreeMap::new()));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let oversized_download = Arc::new(AtomicU8::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let packages_for_server = Arc::clone(&packages);
        let requests_for_server = Arc::clone(&requests);
        let oversized_for_server = Arc::clone(&oversized_download);
        let stop_for_server = Arc::clone(&stop);
        let server = thread::spawn(move || {
            while !stop_for_server.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        serve_request(stream, &packages_for_server, &requests_for_server, &oversized_for_server)
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("local registry accept failed: {error}"),
                }
            }
        });

        let app = root.path().join("App");
        fs::create_dir_all(app.join("Src")).expect("create app source");
        fs::write(app.join("Src/Main.bd"), "Fn Main() { }\n").expect("write app source");
        fs::write(
            root.path().join("Root.bws"),
            format!(
                "workspace {{\n  name = \"Root\"\n}}\n\nmember \"App\" {{\n  path = \"App\"\n}}\n\nregistry \"default\" {{\n  url = \"{url}\"\n}}\n"
            ),
        )
        .expect("write workspace manifest");
        let app_manifest = app.join("App.bproj");
        fs::write(
            &app_manifest,
            "App {\n  name = \"App\"\n  version = \"0.1.0\"\n}\n\ntarget \"App\" {\n  kind = \"App\"\n  entry = \"Main.bd\"\n}\n\ndependency \"PkgCore\" {\n  source = \"registry\"\n  version = \"*\"\n  registry = \"default\"\n}\n",
        )
        .expect("write app manifest");

        Self { root, app_manifest, packages, requests, oversized_download, stop, server: Some(server) }
    }

    fn publish(&self, version: &str, marker: &str) {
        self.packages.lock().expect("package map").insert(version.to_owned(), package_zip(marker));
    }

    fn publish_with_source_dir(&self, version: &str, marker: &str, source_dir: &str) {
        self.packages
            .lock()
            .expect("package map")
            .insert(version.to_owned(), package_zip_with_source_dir(marker, source_dir));
    }

    fn withdraw(&self, version: &str) {
        self.packages.lock().expect("package map").remove(version);
    }

    fn rename_registry_alias(&self, old: &str, new: &str) {
        for path in [self.root.path().join("Root.bws"), self.app_manifest.clone()] {
            let content = fs::read_to_string(&path).expect("read fixture manifest");
            fs::write(&path, content.replace(&format!("\"{old}\""), &format!("\"{new}\"")))
                .expect("rewrite fixture registry alias");
        }
    }

    fn lock_path(&self) -> PathBuf {
        self.app_manifest.parent().expect("app directory").join(PROJECT_LOCK_FILE_NAME)
    }

    fn prepare(&self, refresh_lock: bool) -> Result<beskid_analysis::projects::PreparedProjectWorkspace, ProjectError> {
        with_cwd_at_workspace_root(self.root.path(), || {
            let plan = build_compile_plan_with_policy(&self.app_manifest, None, UnresolvedDependencyPolicy::Warn)?;
            prepare_project_workspace_with_options(
                &plan,
                WorkspacePrepareOptions { frozen: false, locked: false, refresh_lock },
                None,
            )
        })
    }

    fn lock(&self) -> String {
        fs::read_to_string(self.lock_path()).expect("read Project.lock")
    }
}

impl Drop for RegistryFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(server) = self.server.take() {
            server.join().expect("local registry server");
        }
    }
}

fn serve_request(
    mut stream: TcpStream,
    packages: &Mutex<BTreeMap<String, Vec<u8>>>,
    requests: &Mutex<Vec<String>>,
    oversized_download: &AtomicU8,
) {
    stream.set_read_timeout(Some(Duration::from_secs(2))).expect("set request timeout");
    stream.set_write_timeout(Some(Duration::from_secs(2))).expect("set response timeout");
    let mut request = [0_u8; 4096];
    let read = stream.read(&mut request).expect("read registry request");
    let first_line = String::from_utf8_lossy(&request[..read]);
    let path = first_line.split_whitespace().nth(1).unwrap_or("");
    requests.lock().expect("request paths").push(path.to_owned());
    let package_path = "/api/packages/PkgCore/versions";
    if path.ends_with("/download") {
        match oversized_download.load(Ordering::Relaxed) {
            1 => {
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {OVER_LIMIT_BYTES}\r\nConnection: close\r\n\r\n")
                    .expect("write oversized response headers");
                return;
            }
            2 => {
                stream
                    .write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n")
                    .expect("write streamed response headers");
                let chunk = [0_u8; 16 * 1024];
                for _ in 0..=((64 * 1024 * 1024) / chunk.len()) {
                    if stream.write_all(&chunk).is_err() {
                        break;
                    }
                }
                return;
            }
            _ => {}
        }
    }
    let (status, content_type, body) = if path == package_path {
        let versions: Vec<_> = packages.lock().expect("package map").keys().rev().cloned().collect();
        let json = serde_json::to_vec(
            &versions
                .iter()
                .map(|version| serde_json::json!({"version": version, "isYanked": false}))
                .collect::<Vec<_>>(),
        )
        .expect("serialize registry catalog");
        ("200 OK", "application/json", json)
    } else if let Some(version) =
        path.strip_prefix(&format!("{package_path}/")).and_then(|tail| tail.strip_suffix("/download"))
    {
        match packages.lock().expect("package map").get(version) {
            Some(zip) => ("200 OK", "application/zip", zip.clone()),
            None => ("404 Not Found", "text/plain", b"missing version".to_vec()),
        }
    } else {
        ("404 Not Found", "text/plain", b"unknown route".to_vec())
    };
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len(),
    )
    .expect("write registry response headers");
    stream.write_all(&body).expect("write registry response body");
}

// Two stored ZIP entries keep the HTTP artifact real without adding a test dependency.
fn package_zip(marker: &str) -> Vec<u8> {
    package_zip_with_source_dir(marker, "Src")
}

fn package_zip_with_source_dir(marker: &str, source_dir: &str) -> Vec<u8> {
    let source_path = format!("{source_dir}/Marker.bd");
    let files: [(&[u8], &[u8]); 2] = [(b"PkgCore.bproj", PACKAGE_MANIFEST), (source_path.as_bytes(), marker.as_bytes())];
    stored_zip(&files)
}

fn stored_zip(files: &[(&[u8], &[u8])]) -> Vec<u8> {
    let mut zip = Vec::new();
    let mut central = Vec::new();
    for &(name, bytes) in files {
        let offset = zip.len() as u32;
        let crc = crc32(bytes);
        zip.extend_from_slice(&0x0403_4b50_u32.to_le_bytes());
        zip.extend_from_slice(&20_u16.to_le_bytes());
        zip.extend_from_slice(&0_u16.to_le_bytes());
        zip.extend_from_slice(&0_u16.to_le_bytes());
        zip.extend_from_slice(&0_u16.to_le_bytes());
        zip.extend_from_slice(&0_u16.to_le_bytes());
        zip.extend_from_slice(&crc.to_le_bytes());
        zip.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        zip.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        zip.extend_from_slice(&(name.len() as u16).to_le_bytes());
        zip.extend_from_slice(&0_u16.to_le_bytes());
        zip.extend_from_slice(name);
        zip.extend_from_slice(bytes);

        central.extend_from_slice(&0x0201_4b50_u32.to_le_bytes());
        central.extend_from_slice(&20_u16.to_le_bytes());
        central.extend_from_slice(&20_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        central.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u32.to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name);
    }
    let central_offset = zip.len() as u32;
    zip.extend_from_slice(&central);
    zip.extend_from_slice(&0x0605_4b50_u32.to_le_bytes());
    zip.extend_from_slice(&0_u16.to_le_bytes());
    zip.extend_from_slice(&0_u16.to_le_bytes());
    let file_count = u16::try_from(files.len()).expect("fixture ZIP entry count fits u16");
    zip.extend_from_slice(&file_count.to_le_bytes());
    zip.extend_from_slice(&file_count.to_le_bytes());
    zip.extend_from_slice(&(central.len() as u32).to_le_bytes());
    zip.extend_from_slice(&central_offset.to_le_bytes());
    zip.extend_from_slice(&0_u16.to_le_bytes());
    zip
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320_u32 & (0_u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}

fn marker_path(workspace: &beskid_analysis::projects::PreparedProjectWorkspace) -> &Path {
    &workspace
        .materialized_dependencies
        .iter()
        .find(|dependency| dependency.dependency_name == "PkgCore")
        .expect("registry package materialized")
        .materialized_source_root
}

fn registry_destination_paths(fixture: &RegistryFixture) -> Vec<PathBuf> {
    let deps = fixture.app_manifest.parent().unwrap().join("obj/beskid/deps/src");
    let Ok(entries) = fs::read_dir(deps) else { return Vec::new() };
    entries
        .map(|entry| entry.expect("read materialized dependency"))
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("PkgCore-"))
        .map(|entry| entry.path())
        .collect()
}

fn assert_no_registry_staging_dirs(fixture: &RegistryFixture) {
    let deps = fixture.app_manifest.parent().unwrap().join("obj/beskid/deps/src");
    let Ok(entries) = fs::read_dir(deps) else { return };
    let staging = entries
        .map(|entry| entry.expect("read materialized dependency"))
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(".beskid-registry-stage-"))
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    assert!(staging.is_empty(), "registry staging directories must be cleaned up: {staging:?}");
}

#[test]
fn pinned_older_version_survives_newer_registry_release() {
    let fixture = RegistryFixture::new();
    fixture.publish(OLD_VERSION, "old release");
    let first = fixture.prepare(false).expect("initial package lock");
    assert_eq!(fs::read_to_string(marker_path(&first).join("Marker.bd")).unwrap(), "old release");
    let original_lock = fixture.lock();
    fixture.publish(NEW_VERSION, "new release");

    let pinned = fixture.prepare(false).expect("replay pinned package");
    assert_eq!(fs::read_to_string(marker_path(&pinned).join("Marker.bd")).unwrap(), "old release");
    assert_eq!(fixture.lock(), original_lock);
    assert!(original_lock.contains("resolved_version=1.0.0"));
    assert!(
        original_lock
            .contains("artifact_digest=sha256:95922158451951391cb09f7d264c61d705e6aa71ddb7feb42fba8115d8e5e191"),
        "lock: {original_lock}"
    );
}

#[test]
fn changed_manifest_requested_version_rejects_stale_registry_pin_before_materialization() {
    let fixture = RegistryFixture::new();
    fixture.publish(OLD_VERSION, "old release");
    let first = fixture.prepare(false).expect("initial version pin");
    let original_lock = fixture.lock();
    let materialized_root = marker_path(&first).parent().expect("materialized package root");
    let manifest_sentinel = b"materialized manifest sentinel";
    fs::write(materialized_root.join("PkgCore.bproj"), manifest_sentinel).expect("guard materialized package");
    let deps_root = fixture.app_manifest.parent().unwrap().join("obj/beskid/deps/src");
    let mut original_destinations = fs::read_dir(&deps_root)
        .expect("read materialized destinations")
        .map(|entry| entry.expect("read materialized entry").file_name())
        .collect::<Vec<_>>();
    original_destinations.sort();

    fixture.publish(NEW_VERSION, "new release");
    let manifest = fs::read_to_string(&fixture.app_manifest).expect("read app manifest");
    assert!(manifest.contains("version = \"*\""), "fixture dependency must start unconstrained");
    fs::write(&fixture.app_manifest, manifest.replace("version = \"*\"", "version = \"2.0.0\""))
        .expect("request a different exact version");
    let result = fixture.prepare(false);

    assert_eq!(fixture.lock(), original_lock, "stale pin rejection must not rewrite the lock");
    assert_eq!(fs::read(materialized_root.join("PkgCore.bproj")).unwrap(), manifest_sentinel);
    let mut final_destinations = fs::read_dir(&deps_root)
        .expect("read materialized destinations")
        .map(|entry| entry.expect("read materialized entry").file_name())
        .collect::<Vec<_>>();
    final_destinations.sort();
    assert_eq!(final_destinations, original_destinations, "stale pin must not materialize another package");
    let error = result.expect_err("a v2 pin must not override the manifest's exact requested version");
    assert!(
        error.to_string().contains("version") || error.to_string().contains("pin"),
        "unexpected error: {error}"
    );
}

#[test]
fn repeated_pinned_prepare_reuses_identical_materialization_without_rewriting_it() {
    let fixture = RegistryFixture::new();
    fixture.publish(OLD_VERSION, "trusted release");
    let first = fixture.prepare(false).expect("initial materialization");
    let marker = marker_path(&first).join("Marker.bd");
    let original_bytes = fs::read(&marker).expect("read materialized marker");
    let original_modified = fs::metadata(&marker).unwrap().modified().unwrap();
    let original_lock = fixture.lock();
    thread::sleep(Duration::from_millis(1200));

    fixture.prepare(false).expect("reuse identical pinned package");

    assert_eq!(fs::read(&marker).unwrap(), original_bytes);
    assert_eq!(fs::metadata(&marker).unwrap().modified().unwrap(), original_modified);
    assert_eq!(fixture.lock(), original_lock);
}

#[test]
fn tampered_materialization_fails_without_repairing_or_removing_it() {
    let fixture = RegistryFixture::new();
    fixture.publish(OLD_VERSION, "trusted release");
    let first = fixture.prepare(false).expect("initial materialization");
    let marker = marker_path(&first).join("Marker.bd");
    fs::write(&marker, b"local tamper sentinel").expect("tamper with generated package");
    let original_lock = fixture.lock();

    let error = fixture.prepare(false).expect_err("tampered materialization must fail closed");

    assert!(error.to_string().contains("tampered"), "unexpected error: {error}");
    assert_eq!(fs::read(&marker).unwrap(), b"local tamper sentinel");
    assert_eq!(fixture.lock(), original_lock);
}

#[test]
fn extra_materialization_file_fails_without_removing_it() {
    let fixture = RegistryFixture::new();
    fixture.publish(OLD_VERSION, "trusted release");
    let first = fixture.prepare(false).expect("initial materialization");
    let root = marker_path(&first).parent().expect("materialized package root");
    let extra = root.join("extra-local-file.txt");
    fs::write(&extra, b"preserve me").expect("add extra local file");
    let original_lock = fixture.lock();

    let error = fixture.prepare(false).expect_err("extra materialized path must fail closed");

    assert!(error.to_string().contains("tampered"), "unexpected error: {error}");
    assert_eq!(fs::read(&extra).unwrap(), b"preserve me");
    assert_eq!(fixture.lock(), original_lock);
}

#[test]
fn unavailable_unpinned_registry_remains_warning_only() {
    let fixture = RegistryFixture::new();
    let prepared = fixture.prepare(false).expect("unavailable unpinned registry remains unresolved");
    assert!(prepared.materialized_dependencies.iter().all(|dependency| dependency.dependency_name != "PkgCore"));
    assert!(!fixture.lock().contains("name=PkgCore"));
}

#[test]
fn unavailable_unpinned_registry_can_retry_without_rewriting_v2_lock() {
    let fixture = RegistryFixture::new();
    fixture.prepare(false).expect("initial unavailable registry warning");
    let original_lock = fixture.lock();
    let obj = fixture.app_manifest.parent().unwrap().join("obj");
    let original_obj = snapshot_tree(&obj);

    let retry = fixture.prepare(false).expect("unavailable registry must remain warning-only on retry");
    assert!(retry.materialized_dependencies.iter().all(|dependency| dependency.dependency_name != "PkgCore"));
    assert_eq!(fixture.lock(), original_lock);
    assert_eq!(snapshot_tree(&obj), original_obj);

    fixture.publish(OLD_VERSION, "newly available release");
    let error = fixture.prepare(false).expect_err("ordinary prepare must not silently add a new registry pin");
    assert!(error.to_string().contains("beskid update"), "unexpected error: {error}");
    assert_eq!(fixture.lock(), original_lock);
    assert_eq!(snapshot_tree(&obj), original_obj);
}

#[test]
fn strict_preparation_rejects_unpinned_registry_without_mutation() {
    let fixture = RegistryFixture::new();
    fixture.prepare(false).expect("initial unavailable registry warning");
    let original_lock = fixture.lock();
    let obj = fixture.app_manifest.parent().unwrap().join("obj");
    let original_obj = snapshot_tree(&obj);

    for (locked, frozen) in [(true, false), (false, true)] {
        let error = with_cwd_at_workspace_root(fixture.root.path(), || {
            let plan = build_compile_plan_with_policy(&fixture.app_manifest, None, UnresolvedDependencyPolicy::Warn)?;
            prepare_project_workspace_with_options(
                &plan,
                WorkspacePrepareOptions { locked, frozen, refresh_lock: false },
                None,
            )
        })
        .expect_err("strict mode must require every declared registry pin");
        assert!(error.to_string().contains("pin"), "unexpected error: {error}");
        assert_eq!(fixture.lock(), original_lock);
        assert_eq!(snapshot_tree(&obj), original_obj);
    }
}

#[test]
fn strict_preparation_rejects_forged_registry_paths_before_creating_output() {
    for (field, forged) in [
        ("project", "obj/beskid/deps/src/Forged"),
        ("manifest", "Forged.bproj"),
        ("source_root", "Forged"),
        ("materialized_root", "obj/beskid/deps/src/Forged"),
    ] {
        for (locked, frozen) in [(true, false), (false, true)] {
            let fixture = RegistryFixture::new();
            fixture.publish(OLD_VERSION, "trusted release");
            fixture.prepare(false).expect("create a valid digest-pinned v2 lock");
            let original_lock = fixture.lock();
            let original_line = original_lock
                .lines()
                .find(|line| line.starts_with("- name=PkgCore;"))
                .expect("registry lock entry");
            let field_prefix = format!(";{field}=");
            let original_value = original_line
                .split_once(&field_prefix)
                .expect("registry field")
                .1
                .split(';')
                .next()
                .expect("registry field value");
            assert_ne!(original_value, forged, "test must change the {field} field");
            let forged_line = original_line.replace(
                &format!("{field_prefix}{original_value}"),
                &format!("{field_prefix}{forged}"),
            );
            let forged_lock = original_lock.replace(original_line, &forged_line);
            fs::write(fixture.lock_path(), &forged_lock).expect("write forged, parseable v2 lock");

            let output = fixture.app_manifest.parent().unwrap().join("obj");
            fs::remove_dir_all(&output).expect("remove disposable fixture output");
            let error = with_cwd_at_workspace_root(fixture.root.path(), || {
                let plan = build_compile_plan_with_policy(
                    &fixture.app_manifest,
                    None,
                    UnresolvedDependencyPolicy::Warn,
                )?;
                prepare_project_workspace_with_options(
                    &plan,
                    WorkspacePrepareOptions { locked, frozen, refresh_lock: false },
                    None,
                )
            })
            .expect_err("forged registry identity must be rejected");

            assert!(!output.exists(), "forged {field} must fail before any obj output");
            assert!(
                error.to_string().contains("stale") || error.to_string().contains("pin"),
                "unexpected {field} strict-mode error: {error}"
            );
            assert_eq!(fixture.lock(), forged_lock, "rejected {field} lock must remain untouched");
            assert_no_registry_scratch(&fixture);
        }
    }
}

#[test]
fn registry_source_root_tracks_literal_archive_directory_case() {
    for (source_dir, expected_root) in [("src", "src"), ("Src", "Src"), ("SRC", ".")] {
        let fixture = RegistryFixture::new();
        fixture.publish_with_source_dir(OLD_VERSION, "literal source", source_dir);
        let prepared = fixture.prepare(false).expect("prepare registry package");
        let materialized = prepared
            .materialized_dependencies
            .iter()
            .find(|dependency| dependency.dependency_name == "PkgCore")
            .expect("materialized registry package");
        let expected_path = if expected_root == "." {
            materialized.materialized_project_root.clone()
        } else {
            materialized.materialized_project_root.join(expected_root)
        };
        assert_eq!(materialized.materialized_source_root, expected_path, "archive dir {source_dir}");
        assert!(
            fixture.lock().contains(&format!(";source_root={expected_root};")),
            "archive dir {source_dir} produced unexpected lock: {}",
            fixture.lock()
        );
        fixture.prepare(false).expect("replay literal source-root pin");
    }
}

fn assert_no_registry_mutation(fixture: &RegistryFixture) {
    let project = fixture.app_manifest.parent().expect("project root");
    assert!(!project.join("obj").exists(), "oversized artifact must not create obj");
    assert!(!fixture.lock_path().exists(), "oversized artifact must not write a lock");
    assert_no_registry_scratch(fixture);
}

fn assert_no_registry_scratch(fixture: &RegistryFixture) {
    let project = fixture.app_manifest.parent().expect("project root");
    assert!(
        fs::read_dir(project)
            .expect("read project root")
            .flatten()
            .all(|entry| { !entry.file_name().to_string_lossy().starts_with(".beskid-registry-artifact-") }),
        "registry scratch file must be removed"
    );
}

fn snapshot_tree(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    fn visit(root: &Path, current: &Path, snapshot: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
        for entry in fs::read_dir(current).expect("read prepared output") {
            let entry = entry.expect("read prepared output entry");
            let path = entry.path();
            let relative = path.strip_prefix(root).expect("snapshot path is under output root").to_path_buf();
            let kind = entry.file_type().expect("read prepared output type");
            if kind.is_dir() {
                snapshot.insert(relative, None);
                visit(root, &path, snapshot);
            } else {
                assert!(kind.is_file(), "prepared output must not contain a symlink");
                snapshot.insert(relative, Some(fs::read(path).expect("read prepared output file")));
            }
        }
    }

    let mut snapshot = BTreeMap::new();
    visit(root, root, &mut snapshot);
    snapshot
}

#[test]
fn declared_oversized_registry_artifact_rejects_before_any_materialization() {
    let fixture = RegistryFixture::new();
    fixture.publish(OLD_VERSION, "small valid release");
    fixture.oversized_download.store(1, Ordering::Relaxed);
    let error = fixture.prepare(false).expect_err("oversized registry artifact must fail closed");
    assert!(error.to_string().contains("64 MiB"), "unexpected error: {error}");
    assert_no_registry_mutation(&fixture);
}

#[test]
fn streamed_oversized_registry_artifact_rejects_and_cleans_scratch() {
    let fixture = RegistryFixture::new();
    fixture.publish(OLD_VERSION, "small valid release");
    fixture.oversized_download.store(2, Ordering::Relaxed);
    let error = fixture.prepare(false).expect_err("streamed oversized artifact must fail closed");
    assert!(error.to_string().contains("64 MiB"), "unexpected error: {error}");
    assert_no_registry_mutation(&fixture);
}

#[test]
fn pinned_streamed_oversize_preserves_existing_lock_and_prepared_output() {
    let fixture = RegistryFixture::new();
    fixture.publish(OLD_VERSION, "trusted release");
    fixture.prepare(false).expect("initial package lock");
    let original_lock = fixture.lock();
    let obj = fixture.app_manifest.parent().unwrap().join("obj");
    let original_obj = snapshot_tree(&obj);
    fixture.publish(NEW_VERSION, "new release");
    fixture.requests.lock().expect("request paths").clear();
    fixture.oversized_download.store(2, Ordering::Relaxed);

    let error = fixture.prepare(false).expect_err("oversized pinned artifact must fail closed");
    assert!(error.to_string().contains("64 MiB"), "unexpected error: {error}");
    let requests = fixture.requests.lock().expect("request paths");
    assert!(
        requests.iter().any(|path| path.ends_with("/versions/1.0.0/download")),
        "ordinary prepare must request the pinned version: {requests:?}"
    );
    assert!(!requests.iter().any(|path| path.ends_with("/versions/2.0.0/download")));
    assert_eq!(fixture.lock(), original_lock, "failed pinned preflight must preserve the lock");
    assert_eq!(snapshot_tree(&obj), original_obj, "failed pinned preflight must preserve obj");
    assert_no_registry_scratch(&fixture);
}

#[test]
fn refresh_streamed_oversize_preserves_existing_lock_and_prepared_output() {
    let fixture = RegistryFixture::new();
    fixture.publish(OLD_VERSION, "trusted release");
    fixture.prepare(false).expect("initial package lock");
    let original_lock = fixture.lock();
    let obj = fixture.app_manifest.parent().unwrap().join("obj");
    let original_obj = snapshot_tree(&obj);
    fixture.publish(NEW_VERSION, "new release");
    fixture.requests.lock().expect("request paths").clear();
    fixture.oversized_download.store(2, Ordering::Relaxed);

    let error = fixture.prepare(true).expect_err("oversized refresh artifact must fail closed");
    assert!(error.to_string().contains("64 MiB"), "unexpected error: {error}");
    let requests = fixture.requests.lock().expect("request paths");
    assert!(
        requests.iter().any(|path| path == "/api/packages/PkgCore/versions"),
        "refresh must query versions: {requests:?}"
    );
    assert!(
        requests.iter().any(|path| path.ends_with("/versions/2.0.0/download")),
        "refresh must request the newer version: {requests:?}"
    );
    assert_eq!(fixture.lock(), original_lock, "failed refresh must preserve the lock");
    assert_eq!(snapshot_tree(&obj), original_obj, "failed refresh must preserve obj");
    assert_no_registry_scratch(&fixture);
}

#[test]
fn changed_pinned_zip_bytes_fail_before_extraction() {
    let fixture = RegistryFixture::new();
    fixture.publish(OLD_VERSION, "trusted release");
    fixture.prepare(false).expect("initial package lock");
    fixture.publish(OLD_VERSION, "tampered release");
    let error = fixture.prepare(false).expect_err("changed artifact must fail digest check");

    assert!(error.to_string().contains("digest"), "unexpected error: {error}");
    let extracted = fixture.app_manifest.parent().unwrap().join("obj/beskid/deps/src");
    assert!(!tree_contains(&extracted, b"tampered release"));
}

#[cfg(unix)]
#[test]
fn preexisting_nested_symlink_cannot_redirect_registry_extraction() {
    use std::os::unix::fs::symlink;

    let fixture = RegistryFixture::new();
    fixture.publish(OLD_VERSION, "trusted release");
    let first = fixture.prepare(false).expect("initial package materialization");
    let materialized_source = marker_path(&first).to_path_buf();
    let materialized_root = materialized_source.parent().expect("materialized package root");
    let original_source = materialized_root.join("Src-before-symlink");
    fs::rename(&materialized_source, &original_source).expect("preserve materialized source");

    let outside = fixture.root.path().join("Outside");
    fs::create_dir(&outside).expect("create outside sentinel directory");
    let outside_marker = outside.join("Marker.bd");
    fs::write(&outside_marker, b"outside sentinel").expect("write outside sentinel");
    symlink(&outside, &materialized_source).expect("redirect nested archive entry outside materialization");

    let original_lock = fixture.lock();
    let original_manifest = b"materialized manifest sentinel";
    fs::write(materialized_root.join("PkgCore.bproj"), original_manifest).expect("guard first archive entry");
    let original_marker = fs::read(original_source.join("Marker.bd")).expect("read preserved source");
    let result = fixture.prepare(false);

    assert_eq!(fs::read(&outside_marker).expect("read outside sentinel"), b"outside sentinel");
    assert_eq!(fixture.lock(), original_lock, "rejected extraction must not rewrite the lock");
    assert_eq!(fs::read(materialized_root.join("PkgCore.bproj")).unwrap(), original_manifest);
    assert_eq!(fs::read(original_source.join("Marker.bd")).unwrap(), original_marker);
    assert!(
        fs::symlink_metadata(&materialized_source).expect("read nested link").file_type().is_symlink(),
        "rejected extraction must not replace the pre-existing symlink"
    );
    let error = result.expect_err("nested symlink must be rejected before any archive entry is extracted");
    assert!(error.to_string().contains("symlink"), "unexpected error: {error}");
}

#[cfg(unix)]
#[test]
fn preexisting_file_symlink_cannot_redirect_registry_extraction() {
    use std::os::unix::fs::symlink;

    let fixture = RegistryFixture::new();
    fixture.publish(OLD_VERSION, "trusted release");
    let first = fixture.prepare(false).expect("initial package materialization");
    let materialized_root = marker_path(&first).parent().expect("materialized package root");
    let manifest = materialized_root.join("PkgCore.bproj");
    let original_manifest = materialized_root.join("PkgCore-before-symlink.bproj");
    fs::rename(&manifest, &original_manifest).expect("preserve materialized manifest");
    let outside_manifest = fixture.root.path().join("outside-manifest.bproj");
    fs::write(&outside_manifest, b"outside manifest sentinel").expect("write outside sentinel");
    symlink(&outside_manifest, &manifest).expect("redirect archive file outside materialization");
    let original_lock = fixture.lock();

    let result = fixture.prepare(false);

    assert_eq!(fs::read(&outside_manifest).unwrap(), b"outside manifest sentinel");
    assert_eq!(fixture.lock(), original_lock);
    assert!(fs::symlink_metadata(&manifest).unwrap().file_type().is_symlink());
    let error = result.expect_err("file symlink must be rejected before extraction");
    assert!(error.to_string().contains("symlink"), "unexpected error: {error}");
}

#[test]
fn zip_symlink_entry_is_rejected_before_any_archive_file_is_written() {
    let fixture = RegistryFixture::new();
    let mut archive = package_zip("symlink entry payload");
    let central_entries = archive
        .windows(4)
        .enumerate()
        .filter_map(|(offset, bytes)| (bytes == 0x0201_4b50_u32.to_le_bytes()).then_some(offset))
        .collect::<Vec<_>>();
    assert_eq!(central_entries.len(), 2, "fixture ZIP must have two central entries");
    let marker_entry = central_entries[1];
    archive[marker_entry + 4..marker_entry + 6].copy_from_slice(&0x0314_u16.to_le_bytes());
    archive[marker_entry + 38..marker_entry + 42].copy_from_slice(&(0o120777_u32 << 16).to_le_bytes());
    fixture.packages.lock().unwrap().insert(OLD_VERSION.to_owned(), archive);

    let error = fixture.prepare(false).expect_err("ZIP symlink entry must be rejected");

    assert!(error.to_string().contains("symlink"), "unexpected error: {error}");
    assert!(registry_destination_paths(&fixture).is_empty(), "a later ZIP symlink must block publication of the whole package");
}

#[test]
fn zip_directory_mode_on_file_entry_is_rejected_before_publication() {
    let fixture = RegistryFixture::new();
    let mut archive = package_zip("mismatched ZIP mode");
    let central_entries = archive
        .windows(4)
        .enumerate()
        .filter_map(|(offset, bytes)| (bytes == 0x0201_4b50_u32.to_le_bytes()).then_some(offset))
        .collect::<Vec<_>>();
    assert_eq!(central_entries.len(), 2, "fixture ZIP must have two central entries");
    let marker_entry = central_entries[1];
    archive[marker_entry + 4..marker_entry + 6].copy_from_slice(&0x0314_u16.to_le_bytes());
    archive[marker_entry + 38..marker_entry + 42].copy_from_slice(&(0o040755_u32 << 16).to_le_bytes());
    fixture.packages.lock().unwrap().insert(OLD_VERSION.to_owned(), archive);

    let error = fixture.prepare(false).expect_err("ZIP directory mode on a file must be rejected");

    assert!(error.to_string().contains("special ZIP entry"), "unexpected error: {error}");
    assert!(registry_destination_paths(&fixture).is_empty(), "mismatched ZIP type must not publish a package");
    assert_no_registry_staging_dirs(&fixture);
}

#[test]
fn conflicting_zip_entries_leave_no_materialized_package() {
    let fixture = RegistryFixture::new();
    let artifact = stored_zip(&[
        (b"PkgCore.bproj", PACKAGE_MANIFEST),
        (b"Collision", b"ordinary file"),
        (b"Collision/child.bd", b"nested file"),
    ]);
    fixture.packages.lock().unwrap().insert(OLD_VERSION.to_owned(), artifact);

    let error = fixture.prepare(false).expect_err("ZIP file/directory conflict must fail");

    assert!(error.to_string().contains("conflict"), "unexpected error: {error}");
    assert!(registry_destination_paths(&fixture).is_empty(), "conflicting ZIP must not publish a partial package");
    assert_no_registry_staging_dirs(&fixture);
    assert!(!fixture.lock_path().exists(), "rejected ZIP must not write Project.lock");
}

#[test]
fn case_insensitive_zip_aliases_are_rejected_before_publication() {
    let fixture = RegistryFixture::new();
    let artifact = stored_zip(&[
        (b"PkgCore.bproj", PACKAGE_MANIFEST),
        (b"Src/Marker.bd", b"first"),
        (b"src/marker.bd", b"second"),
    ]);
    fixture.packages.lock().unwrap().insert(OLD_VERSION.to_owned(), artifact);

    let error = fixture.prepare(false).expect_err("case-insensitive aliases must not materialize");

    assert!(error.to_string().contains("alias"), "unexpected error: {error}");
    assert!(registry_destination_paths(&fixture).is_empty(), "ambiguous ZIP must not publish a package");
    assert_no_registry_staging_dirs(&fixture);
    assert!(!fixture.lock_path().exists(), "rejected ZIP must not write Project.lock");
}

#[test]
fn unicode_normalization_zip_aliases_are_rejected_before_publication() {
    let fixture = RegistryFixture::new();
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, bytes) in [
        ("PkgCore.bproj", PACKAGE_MANIFEST),
        ("Src/Marker.bd", b"marker".as_slice()),
        ("Src/Caf\u{e9}.bd", b"composed".as_slice()),
        ("Src/Cafe\u{301}.bd", b"decomposed".as_slice()),
    ] {
        writer.start_file(name, stored).expect("start UTF-8 ZIP entry");
        writer.write_all(bytes).expect("write UTF-8 ZIP entry");
    }
    let artifact = writer.finish().expect("finish UTF-8 ZIP").into_inner();
    fixture.packages.lock().unwrap().insert(OLD_VERSION.to_owned(), artifact);

    let error = fixture.prepare(false).expect_err("canonically equivalent paths must not materialize");

    assert!(error.to_string().contains("alias"), "unexpected error: {error}");
    assert!(registry_destination_paths(&fixture).is_empty(), "Unicode aliases must not publish a package");
    assert_no_registry_staging_dirs(&fixture);
    assert!(!fixture.lock_path().exists(), "rejected ZIP must not write Project.lock");
}

fn unicode_alias_zip(first: &str, second: &str) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, bytes) in [
        ("PkgCore.bproj".to_owned(), PACKAGE_MANIFEST),
        ("Src/Marker.bd".to_owned(), b"marker".as_slice()),
        (format!("Src/{first}"), b"first".as_slice()),
        (format!("Src/{second}"), b"second".as_slice()),
    ] {
        writer.start_file(name, stored).expect("start UTF-8 ZIP entry");
        writer.write_all(bytes).expect("write UTF-8 ZIP entry");
    }
    writer.finish().expect("finish UTF-8 ZIP").into_inner()
}

#[test]
fn sigma_and_final_sigma_zip_aliases_are_rejected_before_publication() {
    let fixture = RegistryFixture::new();
    let artifact = unicode_alias_zip("\u{3a3}.bd", "\u{3c2}.bd");
    fixture.packages.lock().unwrap().insert(OLD_VERSION.to_owned(), artifact);

    let error = fixture.prepare(false).expect_err("full Unicode case folding must reject sigma aliases");

    assert!(error.to_string().contains("alias"), "unexpected error: {error}");
    assert!(registry_destination_paths(&fixture).is_empty(), "Unicode aliases must not publish a package");
    assert_no_registry_staging_dirs(&fixture);
}

#[test]
fn sharp_s_and_ss_zip_aliases_are_rejected_before_publication() {
    let fixture = RegistryFixture::new();
    let artifact = unicode_alias_zip("Stra\u{df}e.bd", "STRASSE.bd");
    fixture.packages.lock().unwrap().insert(OLD_VERSION.to_owned(), artifact);

    let error = fixture.prepare(false).expect_err("full Unicode case folding must reject sharp-s aliases");

    assert!(error.to_string().contains("alias"), "unexpected error: {error}");
    assert!(registry_destination_paths(&fixture).is_empty(), "Unicode aliases must not publish a package");
    assert_no_registry_staging_dirs(&fixture);
}

#[test]
fn zip_entry_count_above_ten_thousand_is_rejected_before_publication() {
    let fixture = RegistryFixture::new();
    let mut entries = vec![(b"PkgCore.bproj".as_slice(), PACKAGE_MANIFEST), (b"Src/Marker.bd".as_slice(), b"marker".as_slice())];
    entries.extend(std::iter::repeat_n((b"Repeated/".as_slice(), b"".as_slice()), 9_999));
    let artifact = stored_zip(&entries);
    fixture.packages.lock().unwrap().insert(OLD_VERSION.to_owned(), artifact);

    let error = fixture.prepare(false).expect_err("entry count above ten thousand must be rejected");

    assert!(error.to_string().contains("10,000"), "unexpected error: {error}");
    assert!(registry_destination_paths(&fixture).is_empty(), "over-budget ZIP must not publish a package");
    assert_no_registry_staging_dirs(&fixture);
}

#[test]
fn zip_name_above_four_thousand_ninety_six_utf8_bytes_is_rejected_in_preflight() {
    let fixture = RegistryFixture::new();
    let long_name = format!("Src/{}/Leaf.bd", std::iter::repeat_n("a".repeat(31), 130).collect::<Vec<_>>().join("/"));
    assert!(long_name.len() > 4_096);
    let artifact = stored_zip(&[
        (b"PkgCore.bproj", PACKAGE_MANIFEST),
        (b"Src/Marker.bd", b"marker"),
        (long_name.as_bytes(), b"payload"),
    ]);
    fixture.packages.lock().unwrap().insert(OLD_VERSION.to_owned(), artifact);

    let error = fixture.prepare(false).expect_err("overlong ZIP name must be rejected in preflight");

    assert!(error.to_string().contains("4,096"), "unexpected error: {error}");
    assert!(registry_destination_paths(&fixture).is_empty(), "over-budget ZIP must not publish a package");
    assert_no_registry_staging_dirs(&fixture);
}

#[test]
fn zip_name_above_two_hundred_fifty_six_components_is_rejected_in_preflight() {
    let fixture = RegistryFixture::new();
    let deep_name = format!("Src/{}/Leaf.bd", std::iter::repeat_n("d", 256).collect::<Vec<_>>().join("/"));
    assert!(deep_name.split('/').count() > 256);
    let artifact = stored_zip(&[
        (b"PkgCore.bproj", PACKAGE_MANIFEST),
        (b"Src/Marker.bd", b"marker"),
        (deep_name.as_bytes(), b"payload"),
    ]);
    fixture.packages.lock().unwrap().insert(OLD_VERSION.to_owned(), artifact);

    let error = fixture.prepare(false).expect_err("overdeep ZIP name must be rejected in preflight");

    assert!(error.to_string().contains("256"), "unexpected error: {error}");
    assert!(registry_destination_paths(&fixture).is_empty(), "over-budget ZIP must not publish a package");
    assert_no_registry_staging_dirs(&fixture);
}

#[test]
fn win32_trailing_dot_zip_alias_is_rejected_before_publication() {
    let fixture = RegistryFixture::new();
    let artifact = stored_zip(&[
        (b"PkgCore.bproj", PACKAGE_MANIFEST),
        (b"Src/Marker.bd", b"marker"),
        (b"Src/Readme.bd", b"first"),
        (b"Src/Readme.bd.", b"second"),
    ]);
    fixture.packages.lock().unwrap().insert(OLD_VERSION.to_owned(), artifact);

    let error = fixture.prepare(false).expect_err("Win32-trimmed path must not materialize");

    assert!(error.to_string().contains("non-portable"), "unexpected error: {error}");
    assert!(registry_destination_paths(&fixture).is_empty(), "trailing-dot alias must not publish a package");
    assert_no_registry_staging_dirs(&fixture);
    assert!(!fixture.lock_path().exists(), "rejected ZIP must not write Project.lock");
}

#[test]
fn win32_reserved_device_zip_name_is_rejected_before_publication() {
    let fixture = RegistryFixture::new();
    let artifact = stored_zip(&[
        (b"PkgCore.bproj", PACKAGE_MANIFEST),
        (b"Src/Marker.bd", b"marker"),
        (b"Src/CON.bd", b"device alias"),
    ]);
    fixture.packages.lock().unwrap().insert(OLD_VERSION.to_owned(), artifact);

    let error = fixture.prepare(false).expect_err("Win32 device path must not materialize");

    assert!(error.to_string().contains("non-portable"), "unexpected error: {error}");
    assert!(registry_destination_paths(&fixture).is_empty(), "device alias must not publish a package");
    assert_no_registry_staging_dirs(&fixture);
    assert!(!fixture.lock_path().exists(), "rejected ZIP must not write Project.lock");
}

#[test]
fn zip_directory_with_payload_is_rejected_before_publication() {
    let fixture = RegistryFixture::new();
    let artifact = stored_zip(&[
        (b"PkgCore.bproj", PACKAGE_MANIFEST),
        (b"Src/Marker.bd", b"marker"),
        (b"Src/", b"unread directory payload"),
    ]);
    fixture.packages.lock().unwrap().insert(OLD_VERSION.to_owned(), artifact);

    let error = fixture.prepare(false).expect_err("directory payload must not go unread");

    assert!(error.to_string().contains("directory"), "unexpected error: {error}");
    assert!(registry_destination_paths(&fixture).is_empty(), "invalid directory entry must not publish a package");
    assert_no_registry_staging_dirs(&fixture);
    assert!(!fixture.lock_path().exists(), "rejected ZIP must not write Project.lock");
}

#[test]
fn zip_directory_with_bad_crc_is_rejected_before_publication() {
    let fixture = RegistryFixture::new();
    let mut artifact = stored_zip(&[
        (b"PkgCore.bproj", PACKAGE_MANIFEST),
        (b"Src/Marker.bd", b"marker"),
        (b"Src/", b""),
    ]);
    let directory_central = artifact
        .windows(4)
        .enumerate()
        .filter_map(|(offset, bytes)| (bytes == 0x0201_4b50_u32.to_le_bytes()).then_some(offset))
        .last()
        .expect("directory central entry");
    artifact[directory_central + 16..directory_central + 20].copy_from_slice(&1_u32.to_le_bytes());
    fixture.packages.lock().unwrap().insert(OLD_VERSION.to_owned(), artifact);

    fixture.prepare(false).expect_err("directory CRC must be verified");

    assert!(registry_destination_paths(&fixture).is_empty(), "invalid directory CRC must not publish a package");
    assert_no_registry_staging_dirs(&fixture);
    assert!(!fixture.lock_path().exists(), "rejected ZIP must not write Project.lock");
}

#[test]
fn late_zip_crc_failure_leaves_no_materialized_package() {
    let fixture = RegistryFixture::new();
    let mut artifact = package_zip("late payload");
    let payload = b"late payload";
    let offset = artifact.windows(payload.len()).position(|window| window == payload).expect("find stored ZIP payload");
    artifact[offset] ^= 1;
    fixture.packages.lock().unwrap().insert(OLD_VERSION.to_owned(), artifact);

    fixture.prepare(false).expect_err("late ZIP checksum failure must fail");

    assert!(registry_destination_paths(&fixture).is_empty(), "corrupt ZIP must not publish a partial package");
    assert_no_registry_staging_dirs(&fixture);
    assert!(!fixture.lock_path().exists(), "rejected ZIP must not write Project.lock");
}

#[test]
fn compressed_zip_bomb_exceeding_entry_budget_leaves_no_materialized_package() {
    let fixture = RegistryFixture::new();
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    writer.start_file("PkgCore.bproj", stored).expect("start package manifest");
    writer.write_all(PACKAGE_MANIFEST).expect("write package manifest");
    let deflated = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    writer.start_file("Src/Big.bin", deflated).expect("start compressed test entry");
    let zero_chunk = [0_u8; 64 * 1024];
    for _ in 0..(512 * 1024 * 1024 / zero_chunk.len()) {
        writer.write_all(&zero_chunk).expect("stream compressible test chunk");
    }
    writer.write_all(&[0_u8]).expect("exceed entry budget by one byte");
    let artifact = writer.finish().expect("finish ZIP bomb fixture").into_inner();
    assert!(artifact.len() < 64 * 1024 * 1024, "compressed fixture must pass the compressed cap");
    fixture.packages.lock().unwrap().insert(OLD_VERSION.to_owned(), artifact);

    let error = fixture.prepare(false).expect_err("decompressed entry must be bounded");

    assert!(error.to_string().contains("512 MiB"), "unexpected error: {error}");
    assert!(registry_destination_paths(&fixture).is_empty(), "oversized output must not publish a partial package");
    assert_no_registry_staging_dirs(&fixture);
    assert!(!fixture.lock_path().exists(), "rejected ZIP must not write Project.lock");
}

#[test]
fn missing_pinned_version_fails_without_selecting_newer_release() {
    let fixture = RegistryFixture::new();
    fixture.publish(OLD_VERSION, "old release");
    fixture.prepare(false).expect("initial package lock");
    let original_lock = fixture.lock();
    fixture.withdraw(OLD_VERSION);
    fixture.publish(NEW_VERSION, "new release");

    fixture.prepare(false).expect_err("missing pinned version must fail");
    assert_eq!(fixture.lock(), original_lock);
    let extracted = fixture.app_manifest.parent().unwrap().join("obj/beskid/deps/src");
    assert!(!tree_contains(&extracted, b"new release"));
}

#[test]
fn explicit_update_selects_new_version_and_writes_its_digest() {
    let fixture = RegistryFixture::new();
    fixture.publish(OLD_VERSION, "old release");
    fixture.prepare(false).expect("initial package lock");
    fixture.publish(NEW_VERSION, "new release");

    let updated = fixture.prepare(true).expect("explicit lock refresh");
    assert_eq!(fs::read_to_string(marker_path(&updated).join("Marker.bd")).unwrap(), "new release");
    let lock = fixture.lock();
    assert!(lock.starts_with("# Project.lock v2\n"));
    assert!(lock.contains("resolved_version=2.0.0"));
    assert!(
        lock.contains("artifact_digest=sha256:456bc2d28e9bae140f2462f0e3c6e3fb4721e9b326f893b02c8c846f82033baf"),
        "lock: {lock}"
    );
}

#[test]
fn refresh_rebuilds_pin_after_declared_registry_alias_changes() {
    let fixture = RegistryFixture::new();
    fixture.publish(OLD_VERSION, "old release");
    fixture.prepare(false).expect("initial package lock");
    let original_lock = fixture.lock();
    fixture.rename_registry_alias("default", "other");

    let error = fixture.prepare(false).expect_err("ordinary replay must reject a stale alias");
    assert!(error.to_string().contains("pin"), "unexpected error: {error}");
    assert_eq!(fixture.lock(), original_lock);

    fixture.prepare(true).expect("refresh must use the current registry declaration");
    let updated_lock = fixture.lock();
    assert!(updated_lock.contains("registry=other"));
    assert_ne!(updated_lock, original_lock);
}

#[test]
fn refresh_rebuilds_pins_without_trusting_stale_lock_project_identity() {
    let fixture = RegistryFixture::new();
    fixture.publish(OLD_VERSION, "old release");
    fixture.prepare(false).expect("initial package lock");
    let stale = fixture.lock().replace("project_name=App", "project_name=Other");
    fs::write(fixture.lock_path(), stale).expect("write stale lock identity");

    let error = fixture.prepare(false).expect_err("normal replay must reject stale lock identity");
    assert!(error.to_string().contains("different project"), "unexpected error: {error}");
    fixture.prepare(true).expect("refresh must rebuild current lock identity");
    assert!(fixture.lock().contains("project_name=App"));
}

#[test]
fn duplicate_registry_destination_fails_before_any_materialization() {
    let fixture = RegistryFixture::new();
    fixture.publish(OLD_VERSION, "old release");
    let error = with_cwd_at_workspace_root(fixture.root.path(), || {
        let mut plan = build_compile_plan_with_policy(&fixture.app_manifest, None, UnresolvedDependencyPolicy::Warn)?;
        let duplicate = plan
            .unresolved_dependencies
            .iter()
            .find(|dependency| dependency.dependency_name == "PkgCore")
            .expect("registry dependency in plan")
            .clone();
        plan.unresolved_dependencies.push(duplicate);
        prepare_project_workspace_with_options(&plan, WorkspacePrepareOptions::default(), None)
    })
    .expect_err("duplicate registry destination must fail");

    assert!(error.to_string().contains("materialized destination"), "unexpected error: {error}");
    assert!(!fixture.app_manifest.parent().unwrap().join("obj").exists());
}

#[test]
fn pinned_version_is_one_encoded_url_path_segment() {
    let fixture = RegistryFixture::new();
    fixture.publish(OLD_VERSION, "old release");
    fixture.prepare(false).expect("initial package lock");
    let lock = fixture.lock().replace("resolved_version=1.0.0", "resolved_version=1.0.0/other");
    fs::write(fixture.lock_path(), lock).expect("write malformed version pin");

    fixture.prepare(false).expect_err("malformed pinned version must not fetch a different route");
    let requests = fixture.requests.lock().expect("request paths");
    assert!(
        requests.iter().any(|path| path == "/api/packages/PkgCore/versions/1.0.0%2Fother/download"),
        "request paths: {requests:?}"
    );
}

fn tree_contains(root: &Path, needle: &[u8]) -> bool {
    let Ok(entries) = fs::read_dir(root) else { return false };
    entries.flatten().any(|entry| {
        let path = entry.path();
        if path.is_dir() {
            tree_contains(&path, needle)
        } else {
            fs::read(path).is_ok_and(|bytes| bytes.windows(needle.len()).any(|window| window == needle))
        }
    })
}
