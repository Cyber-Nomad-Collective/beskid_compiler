//! A multi-project workspace reads the toolchain's shared Corelib in place.
//!
//! Completion, definition and diagnostics against the toolchain's shared Corelib in a
//! multi-project workspace; no project receives a Corelib copy. The test reports the workspace
//! scan time and the `obj/beskid/deps` size.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const PROJECTS: usize = 4;

fn send_message(stdin: &mut ChildStdin, message: Value) {
    let body = serde_json::to_vec(&message).expect("serialize LSP message");
    write!(stdin, "Content-Length: {}\r\n\r\n", body.len()).expect("write LSP header");
    stdin.write_all(&body).expect("write LSP body");
    stdin.flush().expect("flush LSP message");
}

fn status_phase(message: &Value) -> Option<&str> {
    (message.get("method").and_then(Value::as_str) == Some("beskid/status"))
        .then(|| message["params"]["phase"].as_str())
        .flatten()
}

fn next(messages: &Receiver<Value>) -> Value {
    messages.recv_timeout(Duration::from_secs(600)).expect("LSP message before timeout")
}

fn response_with_id(messages: &Receiver<Value>, id: i64) -> Value {
    loop {
        let message = next(messages);
        if message.get("id").and_then(Value::as_i64) == Some(id) {
            return message;
        }
    }
}

fn diagnostics_for(messages: &Receiver<Value>, uri: &str) -> Vec<Value> {
    loop {
        let message = next(messages);
        if message.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics")
            && message["params"]["uri"].as_str() == Some(uri)
        {
            return message["params"]["diagnostics"].as_array().cloned().unwrap_or_default();
        }
    }
}

fn directory_bytes(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else { return 0 };
    entries
        .filter_map(Result::ok)
        .map(|entry| {
            let file_type = entry.file_type().expect("file type");
            if file_type.is_dir() { directory_bytes(&entry.path()) } else { entry.metadata().map_or(0, |m| m.len()) }
        })
        .sum()
}

fn write_workspace(folder: &Path) {
    for index in 0..PROJECTS {
        let name = format!("P{index:02}");
        let dir = folder.join(&name);
        std::fs::create_dir_all(dir.join("Src")).expect("project dir");
        std::fs::write(
            dir.join(format!("{name}.bproj")),
            format!(
                "{name} {{\n  name = \"{name}\"\n  version = \"0.1.0\"\n}}\n\ntarget \"App\" {{\n  kind = App\n  entry = \"Main.bd\"\n}}\n"
            ),
        )
        .expect("manifest");
        std::fs::write(
            dir.join("Src/Main.bd"),
            "use Core.Output;\n\ni32 Main() {\n    Output.WriteLine(\"ok\");\n    return 0;\n}\n",
        )
        .expect("source");
    }
}

struct ScanReport {
    deps_bytes: u64,
    corelib_copies: usize,
}

fn spawn_server(corelib_root: &Path, compiler_root: &Path) -> (Child, ChildStdin, Receiver<Value>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_beskid_lsp"))
        .current_dir(compiler_root)
        .env("BESKID_CORELIB_ROOT", corelib_root)
        .env_remove("BESKID_RUNTIME_PREFIX")
        .env_remove("BESKID_RUNTIME_KIT_PROFILE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn beskid_lsp");
    let stdin = child.stdin.take().expect("server stdin");
    let stdout = child.stdout.take().expect("server stdout");
    let (sender, messages) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            let mut content_length = None;
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).ok()? == 0 {
                    return Some(());
                }
                if header == "\r\n" {
                    break;
                }
                if let Some(value) = header.strip_prefix("Content-Length:") {
                    content_length = value.trim().parse::<usize>().ok();
                }
            }
            let mut body = vec![0; content_length?];
            reader.read_exact(&mut body).ok()?;
            sender.send(serde_json::from_slice(&body).ok()?).ok()?;
        }
    });
    (child, stdin, messages)
}

/// Scan a fresh copy of the workspace with `corelib_root`, then exercise completion, diagnostics
/// and definition against Corelib.
fn scan_workspace(label: &str, corelib_root: &Path, shared_root: Option<&Path>) -> ScanReport {
    let compiler_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().and_then(Path::parent).expect("compiler root");
    let parent = compiler_root.join("target");
    std::fs::create_dir_all(&parent).expect("temporary parent");
    let folder = tempfile::tempdir_in(&parent).expect("workspace folder");
    write_workspace(folder.path());
    let folder_uri = url::Url::from_directory_path(folder.path()).expect("folder URI").to_string();
    let mod_host = beskid_tests_mod_host::provisioned_cli().expect("workspace Mod host CLI");
    let runtime_prefix = beskid_tests_mod_host::shared_runtime_prefix().expect("shared runtime kit");

    let (mut child, mut stdin, messages) = spawn_server(corelib_root, compiler_root);
    send_message(
        &mut stdin,
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "processId": null, "rootUri": folder_uri,
            "workspaceFolders": [{"uri": folder_uri, "name": "shared"}],
            "capabilities": {},
            "initializationOptions": {"modHost": {
                "path": mod_host, "runtimePrefix": runtime_prefix, "runtimeProfile": "debug"}}}}),
    );
    assert!(response_with_id(&messages, 1).get("error").is_none());
    let started = Instant::now();
    send_message(&mut stdin, json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    let mut scanning = false;
    loop {
        let message = next(&messages);
        match status_phase(&message) {
            Some("workspace_scan") => scanning = true,
            Some("idle") if scanning => break,
            _ => {}
        }
    }
    let scan = started.elapsed();

    // Completion of a Corelib module member.
    let completion_path = folder.path().join("P00/Src/Main.bd");
    let completion_uri = url::Url::from_file_path(&completion_path).expect("completion URI").to_string();
    let completion_text = "use Core.String;\n\ni32 Main() {\n    String.Sea;\n    return 0;\n}\n";
    send_message(
        &mut stdin,
        json!({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {"textDocument": {
            "uri": completion_uri, "languageId": "beskid", "version": 1, "text": completion_text}}}),
    );
    diagnostics_for(&messages, &completion_uri);
    send_message(
        &mut stdin,
        json!({"jsonrpc": "2.0", "id": 2, "method": "textDocument/completion", "params": {
            "textDocument": {"uri": completion_uri}, "position": {"line": 3, "character": 14}}}),
    );
    let completion = response_with_id(&messages, 2);
    let labels = completion["result"]
        .as_array()
        .expect("completion array")
        .iter()
        .filter_map(|item| item["label"].as_str())
        .collect::<Vec<_>>();
    assert!(labels.contains(&"Search"), "{label}: `String.` must complete from Corelib: {labels:?}");

    // Definition of a Corelib function lands in the Corelib the server reads.
    send_message(
        &mut stdin,
        json!({"jsonrpc": "2.0", "id": 3, "method": "textDocument/definition", "params": {
            "textDocument": {"uri": completion_uri}, "position": {"line": 0, "character": 11}}}),
    );
    let definition = response_with_id(&messages, 3);
    let targets = match &definition["result"] {
        Value::Array(locations) => locations.clone(),
        Value::Null => Vec::new(),
        location => vec![location.clone()],
    };
    for target in &targets {
        let uri = target["uri"].as_str().or_else(|| target["targetUri"].as_str()).unwrap_or_default();
        assert!(!uri.contains("/obj/beskid/deps/"), "{label}: definition points at a project copy: {uri}");
        // A target in the workspace itself (the `use` declaration) is not a Corelib location.
        if uri.starts_with(&folder_uri) {
            continue;
        }
        if let Some(shared) = shared_root {
            let shared_uri = url::Url::from_directory_path(shared).expect("shared URI").to_string();
            assert!(uri.starts_with(&shared_uri), "{label}: definition {uri} is not in the shared Corelib");
        }
    }

    // Diagnostics of a project that misuses Corelib.
    let broken_path = folder.path().join("P01/Src/Main.bd");
    let broken_uri = url::Url::from_file_path(&broken_path).expect("broken URI").to_string();
    send_message(
        &mut stdin,
        json!({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {"textDocument": {
            "uri": broken_uri, "languageId": "beskid", "version": 1,
            "text": "use Core.Output;\n\ni32 Main() {\n    Output.WriteLineMissing(\"ok\");\n    return 0;\n}\n"}}}),
    );
    let mut diagnostics = diagnostics_for(&messages, &broken_uri);
    while diagnostics.is_empty() {
        diagnostics = diagnostics_for(&messages, &broken_uri);
    }
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic["message"].as_str().is_some_and(|m| m.contains("WriteLineMissing"))),
        "{label}: the unknown Corelib member is reported: {diagnostics:?}"
    );
    let _ = child.kill();

    let mut deps_bytes = 0;
    let mut corelib_copies = 0;
    for index in 0..PROJECTS {
        let deps = folder.path().join(format!("P{index:02}/obj/beskid/deps"));
        deps_bytes += directory_bytes(&deps);
        corelib_copies += std::fs::read_dir(deps.join("src")).map(|entries| entries.count()).unwrap_or(0);
    }
    eprintln!(
        "{label}: {PROJECTS} projects, workspace scan {scan:?}, obj/beskid/deps {deps_bytes} bytes, \
         {corelib_copies} dependency copies"
    );
    ScanReport { deps_bytes, corelib_copies }
}

#[test]
fn a_multi_project_workspace_reads_the_shared_corelib_without_copies() {
    let shared = beskid_tests_mod_host::shared_corelib_root().expect("shared provisioned Corelib");
    let shared = shared.canonicalize().expect("canonical shared Corelib");
    let report = scan_workspace("shared Corelib", &shared, Some(&shared));
    assert_eq!(report.corelib_copies, 0, "no project receives a Corelib copy");
    assert_eq!(report.deps_bytes, 0, "no dependency bytes under obj/beskid/deps");
}
