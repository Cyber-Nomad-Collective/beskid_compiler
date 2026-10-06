//! Exercise public commands against a real loopback registry; parsing alone cannot prove offline.
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::Duration;

struct Registry {
    url: String,
    requests: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Registry {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (count, done) = (requests.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            while !done.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        count.fetch_add(1, Ordering::SeqCst);
                        stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                        let _ = stream.read(&mut [0; 8192]);
                        let _ = stream
                            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2))
                    }
                    Err(error) => panic!("loopback accept: {error}"),
                }
            }
        });
        Self { url, requests, stop, thread: Some(thread) }
    }
}
impl Drop for Registry {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.thread.take().unwrap().join().unwrap();
    }
}

fn cli(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_beskid_cli"));
    command
        .current_dir(root)
        .env("BESKID_HOME", root.join("toolchain-home"))
        .env("BESKID_CONFIG_DIR", root.join("config"))
        .env("OTEL_SDK_DISABLED", "true");
    command
}

#[test]
fn v06_offline_cold_registry_fails_before_everyday_command_preparation_without_http() {
    for route in ["check", "build", "run", "test", "doc"] {
        let registry = Registry::new();
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("App/src")).unwrap();
        std::fs::write(root.path().join("Workspace.bws"), format!("workspace {{ name = \"Test\" resolver = v1 }}\nmember \"App\" {{ path = \"App\" }}\nregistry \"local\" {{ url = \"{}\" }}\n", registry.url)).unwrap();
        let manifest = root.path().join("App/App.bproj");
        let source = "App { name = \"App\" version = \"0.1.0\" }\ntarget \"App\" { kind = \"App\" entry = \"Main.bd\" }\ndependency \"Numbers\" { source = \"registry\" version = \"1.0.0\" registry = \"local\" }\n";
        std::fs::write(&manifest, source).unwrap();
        std::fs::write(root.path().join("App/src/Main.bd"), "i32 Main() { return 0; }\n").unwrap();
        let output = cli(root.path()).args([route, "--offline", "--project"]).arg(&manifest).output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(1),
            "{route} must accept offline and report a cache resolution failure, not usage failure: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let diagnostic =
            format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
                .to_ascii_lowercase();
        assert!(
            diagnostic.contains("offline") && (diagnostic.contains("cache") || diagnostic.contains("pin")),
            "{route} must fail at offline dependency resolution, not an unrelated setup/semantic gate: {diagnostic}"
        );
        assert_eq!(
            registry.requests.load(Ordering::SeqCst),
            0,
            "{route} must propagate offline to the shared resolver"
        );
        assert_eq!(std::fs::read_to_string(&manifest).unwrap(), source);
        assert!(!manifest.with_file_name("Project.lock").exists());
        assert!(!manifest.parent().unwrap().join("obj").exists(), "{route} must fail before executable preparation");
        assert!(!root.path().join("doc-out").exists());
    }
}

#[test]
fn v06_new_explicit_offline_uses_bundled_app_without_registry_requests() {
    let registry = Registry::new();
    let root = tempfile::tempdir().unwrap();
    let output =
        cli(root.path()).args(["new", "hello", "--offline", "--registry-url", &registry.url]).output().unwrap();
    assert!(output.status.success(), "bundled offline new: {}", String::from_utf8_lossy(&output.stderr));
    assert!(root.path().join("hello/hello.bproj").is_file());
    assert_eq!(registry.requests.load(Ordering::SeqCst), 0);
}

#[test]
fn v06_new_offline_uncached_registry_template_fails_before_output_without_http() {
    let registry = Registry::new();
    let root = tempfile::tempdir().unwrap();
    let output = cli(root.path())
        .args(["new", "hello", "--offline", "--package", "beskid.templates.app@0.1.0", "--registry-url", &registry.url])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "offline registry template must fail as resolution, not usage: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let diagnostic = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
    assert!(
        diagnostic.contains("offline") && diagnostic.contains("cache"),
        "uncached template failure must identify the offline cache condition: {diagnostic}"
    );
    assert_eq!(registry.requests.load(Ordering::SeqCst), 0);
    assert!(!root.path().join("hello").exists());
}

#[test]
fn v06_locked_direct_mutations_preserve_manifest_lock_and_materialized_bytes() {
    for (route, suffix) in
        [("add", vec!["Other", "--path", "../Other"]), ("remove", vec!["Local"]), ("update", vec!["Local"])]
    {
        let root = tempfile::tempdir().unwrap();
        for name in ["App", "Local", "Other"] {
            std::fs::create_dir_all(root.path().join(name)).unwrap();
            std::fs::write(root.path().join(format!("{name}/{name}.bproj")), format!("{name} {{ name = \"{name}\" version = \"0.1.0\" }}\ntarget \"{name}\" {{ kind = \"Lib\" entry = \"Main.bd\" }}\n")).unwrap();
        }
        let manifest = root.path().join("App/App.bproj");
        let source = "App { name = \"App\" version = \"0.1.0\" }\ntarget \"App\" { kind = \"App\" entry = \"Main.bd\" }\ndependency \"Local\" { source = \"path\" path = \"../Local\" }\n";
        std::fs::write(&manifest, source).unwrap();
        let project = manifest.parent().unwrap();
        std::fs::create_dir_all(project.join("obj/owned")).unwrap();
        std::fs::write(project.join("obj/owned/marker"), "preserve object bytes\n").unwrap();
        // Mutation policy must reject before interpreting/replacing even an existing foreign lock.
        let lock = project.join("Project.lock");
        std::fs::write(&lock, "foreign lock bytes\n").unwrap();
        let output =
            cli(root.path()).arg(route).args(suffix).args(["--locked", "--project"]).arg(&manifest).output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(1),
            "{route} locked mutation: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let diagnostic = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
        assert!(
            diagnostic.contains("locked") && diagnostic.contains("mutation"),
            "mutation policy must reject rather than ignore --locked: {diagnostic}"
        );
        assert_eq!(std::fs::read_to_string(&manifest).unwrap(), source);
        assert_eq!(std::fs::read_to_string(lock).unwrap(), "foreign lock bytes\n");
        assert_eq!(std::fs::read_to_string(project.join("obj/owned/marker")).unwrap(), "preserve object bytes\n");
    }
}

#[test]
fn v06_frozen_direct_add_cold_registry_fails_without_http_or_intent_writes() {
    let registry = Registry::new();
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("App")).unwrap();
    std::fs::write(root.path().join("Workspace.bws"), format!("workspace {{ name = \"Test\" resolver = v1 }}\nmember \"App\" {{ path = \"App\" }}\nregistry \"default\" {{ url = \"{}\" }}\n", registry.url)).unwrap();
    let manifest = root.path().join("App/App.bproj");
    let source = "App { name = \"App\" version = \"0.1.0\" }\ntarget \"App\" { kind = \"App\" entry = \"Main.bd\" }\n";
    std::fs::write(&manifest, source).unwrap();
    let output =
        cli(root.path()).args(["add", "Numbers@1.0.0", "--frozen", "--project"]).arg(&manifest).output().unwrap();
    assert_eq!(output.status.code(), Some(1), "frozen mutation: {}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(registry.requests.load(Ordering::SeqCst), 0);
    assert_eq!(std::fs::read_to_string(&manifest).unwrap(), source);
    assert!(!manifest.with_file_name("Project.lock").exists());
    assert!(!manifest.parent().unwrap().join("obj").exists());
}

fn local_template(root: &Path) -> std::path::PathBuf {
    let template = root.join("local-template");
    std::fs::create_dir_all(template.join(".beskid")).unwrap();
    std::fs::write(
        template.join(".beskid/template.json"),
        r#"{
        "schema":"beskid.template.v1", "identity":"test.offline", "name":"Offline",
        "shortName":"offline-local", "sources":[{"source":"./","target":"./","include":["README.md"]}]
    }"#,
    )
    .unwrap();
    std::fs::write(template.join("README.md"), "verified local bytes\n").unwrap();
    template
}

#[test]
fn v06_new_offline_local_template_resolves_without_network() {
    let registry = Registry::new();
    let root = tempfile::tempdir().unwrap();
    let template = local_template(root.path());
    let output = cli(root.path())
        .args(["new", "local", "--offline", "--no-interactive", "--path"])
        .arg(template)
        .args(["--registry-url", &registry.url])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(std::fs::read_to_string(root.path().join("local/README.md")).unwrap(), "verified local bytes\n");
    assert_eq!(registry.requests.load(Ordering::SeqCst), 0);
}

#[test]
fn v06_new_offline_installed_template_checks_bytes_before_output() {
    let registry = Registry::new();
    let root = tempfile::tempdir().unwrap();
    let template = local_template(root.path());
    let install = cli(root.path())
        .args(["package", "template", "install", "offline-local", "--path"])
        .arg(template)
        .args(["--registry-url", &registry.url])
        .output()
        .unwrap();
    assert!(install.status.success(), "{}", String::from_utf8_lossy(&install.stderr));
    let instantiate = |name: &str| {
        cli(root.path())
            .args([
                "new",
                name,
                "--offline",
                "--no-interactive",
                "--template",
                "offline-local",
                "--registry-url",
                &registry.url,
            ])
            .output()
            .unwrap()
    };
    let warm = instantiate("warm");
    assert!(warm.status.success(), "{}", String::from_utf8_lossy(&warm.stderr));
    assert_eq!(std::fs::read_to_string(root.path().join("warm/README.md")).unwrap(), "verified local bytes\n");
    std::fs::write(root.path().join("config/templates/installed/test.offline/README.md"), "tampered\n").unwrap();
    let tampered = instantiate("tampered");
    assert_eq!(tampered.status.code(), Some(1));
    let diagnostic = String::from_utf8_lossy(&tampered.stderr).to_ascii_lowercase();
    assert!(diagnostic.contains("checksum") || diagnostic.contains("integrity"), "{diagnostic}");
    assert!(!root.path().join("tampered").exists());
    assert_eq!(registry.requests.load(Ordering::SeqCst), 0);
}

#[test]
fn v06_new_offline_rejects_unconstrained_post_action_before_writes() {
    let root = tempfile::tempdir().unwrap();
    let template = local_template(root.path());
    let path = template.join(".beskid/template.json");
    let mut manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    manifest["postActions"] = serde_json::json!([{"actionId":"runCommand","args":{"command":"exit 0"}}]);
    std::fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let output = cli(root.path())
        .args(["new", "unsafe", "--offline", "--no-interactive", "--path"])
        .arg(template)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let diagnostic = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
    assert!(diagnostic.contains("offline") && diagnostic.contains("post-action"), "{diagnostic}");
    assert!(!root.path().join("unsafe").exists());
}

#[test]
fn v06_new_offline_installed_git_metadata_uses_installed_payload_checksum() {
    let root = tempfile::tempdir().unwrap();
    let template = local_template(root.path());
    std::fs::create_dir(template.join(".git")).unwrap();
    std::fs::write(template.join(".git/config"), "source-only metadata\n").unwrap();
    let install = cli(root.path())
        .args(["package", "template", "install", "offline-local", "--path"])
        .arg(template)
        .output()
        .unwrap();
    assert!(install.status.success(), "{}", String::from_utf8_lossy(&install.stderr));
    assert!(!root.path().join("config/templates/installed/test.offline/.git").exists());
    let output = cli(root.path())
        .args(["new", "git-warm", "--offline", "--no-interactive", "--template", "offline-local"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(std::fs::read_to_string(root.path().join("git-warm/README.md")).unwrap(), "verified local bytes\n");
}

#[cfg(unix)]
#[test]
fn v06_template_install_rejects_symlink_payload_and_cycle_before_cache_writes() {
    for cycle in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let template = local_template(root.path());
        let outside = root.path().join("outside.txt");
        std::fs::write(&outside, "external bytes\n").unwrap();
        std::os::unix::fs::symlink(if cycle { &template } else { &outside }, template.join("linked")).unwrap();
        let output = cli(root.path())
            .args(["package", "template", "install", "offline-local", "--path"])
            .arg(template)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(1),
            "symlink payload must be rejected: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let diagnostic = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
        assert!(diagnostic.contains("symlink") || diagnostic.contains("symbolic link"), "{diagnostic}");
        assert!(!root.path().join("config/templates/installed/test.offline").exists());
        assert!(!root.path().join("config/templates/installed/test.offline/manifest.snapshot.json").exists());
    }
}

#[test]
fn v06_new_offline_old_cache_digest_requires_explicit_reinstallation() {
    let root = tempfile::tempdir().unwrap();
    let template = local_template(root.path());
    let output = cli(root.path())
        .args(["package", "template", "install", "offline-local", "--path"])
        .arg(template)
        .output()
        .unwrap();
    assert!(output.status.success());
    let receipt = root.path().join("config/templates/installed/test.offline/manifest.snapshot.json");
    let mut snapshot: serde_json::Value = serde_json::from_slice(&std::fs::read(&receipt).unwrap()).unwrap();
    snapshot["checksum"] = serde_json::json!("0123456789abcdef");
    std::fs::write(receipt, serde_json::to_vec(&snapshot).unwrap()).unwrap();
    let output = cli(root.path())
        .args(["new", "old-cache", "--offline", "--no-interactive", "--template", "offline-local"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let diagnostic = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
    assert!(diagnostic.contains("digest") && diagnostic.contains("reinstall"), "{diagnostic}");
    assert!(!root.path().join("old-cache").exists());
}

#[cfg(unix)]
#[test]
fn v06_template_install_preflight_preserves_existing_verified_cache() {
    let root = tempfile::tempdir().unwrap();
    let template = local_template(root.path());
    let install = || {
        cli(root.path())
            .args(["package", "template", "install", "offline-local", "--path"])
            .arg(&template)
            .output()
            .unwrap()
    };
    assert!(install().status.success());
    let installed = root.path().join("config/templates/installed/test.offline");
    let receipt = std::fs::read(installed.join("manifest.snapshot.json")).unwrap();
    std::fs::write(template.join("README.md"), "replacement must not install\n").unwrap();
    std::os::unix::fs::symlink(root.path().join("absent-external"), template.join("bad-link")).unwrap();
    assert_eq!(install().status.code(), Some(1));
    assert_eq!(std::fs::read(installed.join("manifest.snapshot.json")).unwrap(), receipt);
    assert_eq!(std::fs::read_to_string(installed.join("README.md")).unwrap(), "verified local bytes\n");
}
