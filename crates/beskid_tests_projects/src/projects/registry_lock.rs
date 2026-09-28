//! End-to-end registry pins against a loopback package server.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
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

struct RegistryFixture {
    root: TempDir,
    app_manifest: PathBuf,
    packages: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
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
        let stop = Arc::new(AtomicBool::new(false));
        let packages_for_server = Arc::clone(&packages);
        let stop_for_server = Arc::clone(&stop);
        let server = thread::spawn(move || {
            while !stop_for_server.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => serve_request(stream, &packages_for_server),
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
            "project {\n  name = \"App\"\n  version = \"0.1.0\"\n}\n\ntarget \"App\" {\n  kind = \"App\"\n  entry = \"Main.bd\"\n}\n\ndependency \"PkgCore\" {\n  source = \"registry\"\n  version = \"*\"\n  registry = \"default\"\n}\n",
        )
        .expect("write app manifest");

        Self { root, app_manifest, packages, stop, server: Some(server) }
    }

    fn publish(&self, version: &str, marker: &str) {
        self.packages.lock().expect("package map").insert(version.to_owned(), package_zip(marker));
    }

    fn withdraw(&self, version: &str) {
        self.packages.lock().expect("package map").remove(version);
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

fn serve_request(mut stream: TcpStream, packages: &Mutex<BTreeMap<String, Vec<u8>>>) {
    stream.set_read_timeout(Some(Duration::from_secs(2))).expect("set request timeout");
    stream.set_write_timeout(Some(Duration::from_secs(2))).expect("set response timeout");
    let mut request = [0_u8; 4096];
    let read = stream.read(&mut request).expect("read registry request");
    let first_line = String::from_utf8_lossy(&request[..read]);
    let path = first_line.split_whitespace().nth(1).unwrap_or("");
    let package_path = "/api/packages/PkgCore/versions";
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
    let manifest = b"project {\n  name = \"PkgCore\"\n  version = \"0.1.0\"\n}\n\ntarget \"PkgCore\" {\n  kind = \"Lib\"\n  entry = \"Marker.bd\"\n}\n";
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
    &workspace.materialized_dependencies[0].materialized_source_root
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
            .contains("artifact_digest=sha256:72e03aade1f8c490d74f061c8d4451d5525a08b98a3162dfd4ebe0f3976de653")
    );
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
    assert!(lock.contains("artifact_digest=sha256:44444510a5e1f81deec8851de20e8846c340bbaa40d0cbb6812e58888a1e1b15"));
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
