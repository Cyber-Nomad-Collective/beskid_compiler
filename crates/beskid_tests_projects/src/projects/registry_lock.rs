//! End-to-end registry pins against a loopback package server.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
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
                    Ok((stream, _)) => serve_request(stream, &packages_for_server, &requests_for_server, &oversized_for_server),
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
                stream.write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n")
                    .expect("write streamed response headers");
                let chunk = [0_u8; 16 * 1024];
                for _ in 0..=((64 * 1024 * 1024) / chunk.len()) {
                    if stream.write_all(&chunk).is_err() { break; }
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
    let manifest = b"PkgCore {\n  name = \"PkgCore\"\n  version = \"0.1.0\"\n}\n\ntarget \"PkgCore\" {\n  kind = \"Lib\"\n  entry = \"Marker.bd\"\n}\n";
    let files: [(&[u8], &[u8]); 2] = [(b"PkgCore.bproj", manifest), (b"Src/Marker.bd", marker.as_bytes())];
    let mut zip = Vec::new();
    let mut central = Vec::new();
    for (name, bytes) in files {
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
    zip.extend_from_slice(&2_u16.to_le_bytes());
    zip.extend_from_slice(&2_u16.to_le_bytes());
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
fn unavailable_unpinned_registry_remains_warning_only() {
    let fixture = RegistryFixture::new();
    let prepared = fixture.prepare(false).expect("unavailable unpinned registry remains unresolved");
    assert!(prepared.materialized_dependencies.iter().all(|dependency| dependency.dependency_name != "PkgCore"));
    assert!(!fixture.lock().contains("name=PkgCore"));
}

fn assert_no_registry_mutation(fixture: &RegistryFixture) {
    let project = fixture.app_manifest.parent().expect("project root");
    assert!(!project.join("obj").exists(), "oversized artifact must not create obj");
    assert!(!fixture.lock_path().exists(), "oversized artifact must not write a lock");
    assert!(
        fs::read_dir(project).expect("read project root").flatten().all(|entry| {
            !entry.file_name().to_string_lossy().starts_with(".beskid-registry-artifact-")
        }),
        "registry scratch file must be removed"
    );
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
    assert!(lock.contains("artifact_digest=sha256:456bc2d28e9bae140f2462f0e3c6e3fb4721e9b326f893b02c8c846f82033baf"), "lock: {lock}");
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
        let mut plan = build_compile_plan_with_policy(
            &fixture.app_manifest,
            None,
            UnresolvedDependencyPolicy::Warn,
        )?;
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
