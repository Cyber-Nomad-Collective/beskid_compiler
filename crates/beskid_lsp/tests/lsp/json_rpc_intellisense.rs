use std::io::{BufRead, BufReader, Read, Write};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use serde_json::{Value, json};

fn send_message(stdin: &mut ChildStdin, message: Value) {
    let body = serde_json::to_vec(&message).expect("serialize LSP message");
    write!(stdin, "Content-Length: {}\r\n\r\n", body.len()).expect("write LSP header");
    stdin.write_all(&body).expect("write LSP body");
    stdin.flush().expect("flush LSP message");
}

fn response_with_id(messages: &Receiver<Value>, id: i64) -> Value {
    loop {
        let message = messages
            .recv_timeout(Duration::from_secs(30))
            .unwrap_or_else(|error| panic!("LSP response {id} before timeout: {error}"));
        if message.get("id").and_then(Value::as_i64) == Some(id) {
            return message;
        }
    }
}

fn completion_labels(response: &Value) -> Vec<&str> {
    response["result"].as_array().expect("completion array").iter().filter_map(|item| item["label"].as_str()).collect()
}

fn wait_for_idle_status_with_document_diagnostics(messages: &Receiver<Value>, uri: &str) -> Value {
    let mut matched = None;
    loop {
        let message = messages.recv_timeout(Duration::from_secs(30)).expect("LSP notification before timeout");
        if message.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics")
            && message["params"]["uri"].as_str() == Some(uri)
        {
            matched = Some(message);
            continue;
        }
        if message.get("method").and_then(Value::as_str) == Some("beskid/status")
            && message["params"]["phase"].as_str() == Some("idle")
            && message["params"]["active"].as_bool() == Some(false)
        {
            return matched
                .unwrap_or_else(|| panic!("LSP did not publish diagnostics for closed workspace file {uri}"));
        }
    }
}

fn wait_for_document_diagnostics(messages: &Receiver<Value>, uri: &str, version: i64) {
    loop {
        let message = messages.recv_timeout(Duration::from_secs(30)).expect("LSP diagnostics before timeout");
        if message.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics")
            && message["params"]["uri"].as_str() == Some(uri)
            && message["params"]["version"].as_i64() == Some(version)
        {
            return;
        }
    }
}

#[test]
fn json_rpc_completion_and_hover_use_dependency_syntax_facts() {
    // The Corelib reaches the server only through `BESKID_CORELIB_ROOT`, so the project can live
    // anywhere; it stays outside the source tree.
    let project = tempfile::tempdir().expect("temporary project");
    // URIs name the canonical path, as the server reports it (the system temporary directory may be
    // a symbolic link).
    let project_root = project.path().canonicalize().expect("canonical project directory");
    std::fs::create_dir(project_root.join("Src")).expect("source directory");
    std::fs::write(
        project_root.join("ProtocolSmoke.bproj"),
        "ProtocolSmoke {\n  name = \"ProtocolSmoke\"\n  version = \"0.1.0\"\n}\n\ntarget \"App\" {\n  kind = App\n  entry = \"Main.bd\"\n}\n",
    )
    .expect("project manifest");
    let source = "use Core.Output;\n\ni32 Main() {\n    Output.WriteLine(\"ok\");\n    return 0;\n}\n";
    let source_path = project_root.join("Src/Main.bd");
    std::fs::write(&source_path, source).expect("source file");
    let sibling_source =
        "use Core.Output;\n\ni32 Sibling() {\n    Output.WriteLine(\"ok\");\n    return \"wrong\";\n}\n";
    let sibling_path = project_root.join("Src/Sibling.bd");
    std::fs::write(&sibling_path, sibling_source).expect("sibling source file");

    let project_uri = url::Url::from_directory_path(&project_root).expect("project URI").to_string();
    let source_uri = url::Url::from_file_path(&source_path).expect("source URI").to_string();
    let sibling_uri = url::Url::from_file_path(&sibling_path).expect("sibling URI").to_string();
    // `beskid_lsp` provisions its embedded Corelib into the resolved Corelib root on startup and
    // keeps a sealed root whose fingerprint already matches. The target directory's shared sealed
    // installation of that same embedded bundle is such a root, so an inherited
    // `BESKID_CORELIB_ROOT` (for example the checked-in `corelib/` source tree) is never touched.
    // Preparing a Corelib project runs the native Serialization Mod in a Mod host. The server
    // never builds Mods and this test binary has no installed `beskid` next to it, so the shared
    // root carries Mods provisioned by the workspace CLI against the shared debug runtime kit, and
    // that CLI is selected with that kit as the server's Mod host.
    let mod_host = beskid_tests_mod_host::provisioned_cli().expect("workspace Mod host CLI");
    let runtime_prefix = beskid_tests_mod_host::shared_runtime_prefix().expect("shared runtime kit");
    let corelib_root = beskid_tests_mod_host::shared_corelib_root().expect("shared provisioned Corelib");
    let mut child = Command::new(env!("CARGO_BIN_EXE_beskid_lsp"))
        .current_dir(&project_root)
        .env("BESKID_CORELIB_ROOT", &corelib_root)
        // The runtime kit reaches the server only through `modHost.runtimePrefix`.
        .env_remove("BESKID_RUNTIME_PREFIX")
        .env_remove("BESKID_RUNTIME_KIT_PROFILE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn beskid_lsp");
    let mut stdin = child.stdin.take().expect("server stdin");
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

    send_message(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "processId": null,
                "rootUri": project_uri,
                "workspaceFolders": [{"uri": project_uri, "name": "ProtocolSmoke"}],
                "capabilities": {},
                "initializationOptions": {"modHost": {
                    "path": mod_host,
                    "runtimePrefix": runtime_prefix,
                    "runtimeProfile": "debug"
                }}
            }
        }),
    );
    assert!(response_with_id(&messages, 1).get("error").is_none());
    send_message(&mut stdin, json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    let closed_sibling_diagnostics = wait_for_idle_status_with_document_diagnostics(&messages, &sibling_uri);
    assert!(
        closed_sibling_diagnostics["params"]["diagnostics"].as_array().is_some_and(|diagnostics| diagnostics
            .iter()
            .any(|diagnostic| diagnostic["code"].as_str() == Some("E1206"))),
        "closed workspace file must receive its project type-check diagnostic: {closed_sibling_diagnostics:#}"
    );
    send_message(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {"textDocument": {"uri": source_uri, "languageId": "beskid", "version": 1, "text": source}}
        }),
    );
    wait_for_document_diagnostics(&messages, &source_uri, 1);

    send_message(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/completion",
            "params": {"textDocument": {"uri": source_uri}, "position": {"line": 3, "character": 11}}
        }),
    );
    let completion = response_with_id(&messages, 2);
    let labels = completion_labels(&completion);
    assert!(labels.contains(&"WriteLine"), "expected WriteLine completion, got {labels:?}");

    send_message(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "textDocument/hover",
            "params": {"textDocument": {"uri": source_uri}, "position": {"line": 3, "character": 13}}
        }),
    );
    assert!(!response_with_id(&messages, 3)["result"].is_null(), "expected WriteLine hover");

    send_message(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {"uri": sibling_uri, "languageId": "beskid", "version": 1, "text": sibling_source}
            }
        }),
    );
    wait_for_document_diagnostics(&messages, &sibling_uri, 1);

    let partial_source = source.replace("Main", "FreshMain").replace("WriteLine", "Wri");
    send_message(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": {"uri": source_uri, "version": 2},
                "contentChanges": [{"text": partial_source}]
            }
        }),
    );
    // Ask before the 120 ms diagnostic pass fires. This proves the immediate
    // EntryOnly generation can serve suggestions for semantically partial input.
    send_message(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "textDocument/completion",
            "params": {"textDocument": {"uri": source_uri}, "position": {"line": 3, "character": 14}}
        }),
    );
    let partial_completion = response_with_id(&messages, 4);
    let partial_labels = completion_labels(&partial_completion);
    assert!(
        partial_labels.contains(&"WriteLine"),
        "expected WriteLine completion for partial member input, got {partial_labels:?}"
    );
    std::thread::sleep(Duration::from_millis(250));

    send_message(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "textDocument/documentSymbol",
            "params": {"textDocument": {"uri": source_uri}}
        }),
    );
    let edited_symbols = response_with_id(&messages, 5);
    let edited_symbol_names = edited_symbols["result"]
        .as_array()
        .expect("document symbol array")
        .iter()
        .filter_map(|item| item["name"].as_str())
        .collect::<Vec<_>>();
    assert!(
        edited_symbol_names.contains(&"FreshMain") && !edited_symbol_names.contains(&"Main"),
        "edited document symbols must come from the current generation, got {edited_symbol_names:?}"
    );

    send_message(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "textDocument/completion",
            "params": {"textDocument": {"uri": sibling_uri}, "position": {"line": 3, "character": 14}}
        }),
    );
    let sibling_completion = response_with_id(&messages, 6);
    assert!(
        completion_labels(&sibling_completion).contains(&"WriteLine"),
        "editing another buffer must not invalidate sibling completion facts: {sibling_completion:#}"
    );

    let unimported_sibling = sibling_source.replacen("use Core.Output;\n\n", "", 1);
    send_message(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": {"uri": sibling_uri, "version": 2},
                "contentChanges": [{"text": unimported_sibling}]
            }
        }),
    );
    send_message(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "textDocument/completion",
            "params": {"textDocument": {"uri": sibling_uri}, "position": {"line": 1, "character": 14}}
        }),
    );
    let unimported_completion = response_with_id(&messages, 7);
    assert!(
        !completion_labels(&unimported_completion).contains(&"WriteLine"),
        "deleting an import must clear its owned completion surface"
    );

    send_message(&mut stdin, json!({"jsonrpc": "2.0", "id": 8, "method": "shutdown", "params": null}));
    let _ = response_with_id(&messages, 8);
    send_message(&mut stdin, json!({"jsonrpc": "2.0", "method": "exit", "params": null}));
    drop(stdin);
    assert!(child.wait().expect("wait for server").success());
}

/// The next `publishDiagnostics` for `uri` within `wait`, with its version.
fn next_document_diagnostics(messages: &Receiver<Value>, uri: &str, wait: Duration) -> Option<Value> {
    let deadline = std::time::Instant::now() + wait;
    while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
        let Ok(message) = messages.recv_timeout(left) else { return None };
        if message.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics")
            && message["params"]["uri"].as_str() == Some(uri)
        {
            return Some(message);
        }
    }
    None
}

/// Wait (at most 30 s) until the server's trace holds a line containing `needle`.
fn wait_for_trace(trace: &std::sync::Mutex<Vec<String>>, needle: &str) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        if trace.lock().expect("trace lines").iter().any(|line| line.contains(needle)) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// An edit of an open document is analyzed once and its diagnostics are published once, from
/// that analysis: the debounced full preparation of the document, which repeated the analysis's
/// diagnostics in a second typing of the whole assembly, does not run. Opening the document still
/// runs it (the fallback for a document without an edit analysis).
#[test]
fn an_edit_publishes_its_diagnostics_once_without_a_full_preparation() {
    let project = tempfile::tempdir().expect("temporary project");
    let project_root = project.path().canonicalize().expect("canonical project directory");
    std::fs::create_dir(project_root.join("Src")).expect("source directory");
    std::fs::write(
        project_root.join("EditPublish.bproj"),
        "EditPublish {\n  name = \"EditPublish\"\n  version = \"0.1.0\"\n}\n\ntarget \"App\" {\n  kind = App\n  entry = \"Main.bd\"\n}\n",
    )
    .expect("project manifest");
    let source = "i32 Main() {\n    return 0;\n}\n";
    let source_path = project_root.join("Src/Main.bd");
    std::fs::write(&source_path, source).expect("source file");
    let project_uri = url::Url::from_directory_path(&project_root).expect("project URI").to_string();
    let source_uri = url::Url::from_file_path(&source_path).expect("source URI").to_string();
    let mod_host = beskid_tests_mod_host::provisioned_cli().expect("workspace Mod host CLI");
    let runtime_prefix = beskid_tests_mod_host::shared_runtime_prefix().expect("shared runtime kit");
    let corelib_root = beskid_tests_mod_host::shared_corelib_root().expect("shared provisioned Corelib");
    let mut child = Command::new(env!("CARGO_BIN_EXE_beskid_lsp"))
        .current_dir(&project_root)
        .env("BESKID_CORELIB_ROOT", &corelib_root)
        .env("BESKID_LSP_TRACE", "1")
        .env_remove("BESKID_RUNTIME_PREFIX")
        .env_remove("BESKID_RUNTIME_KIT_PROFILE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn beskid_lsp");
    let mut stdin = child.stdin.take().expect("server stdin");
    let stdout = child.stdout.take().expect("server stdout");
    let stderr = child.stderr.take().expect("server stderr");
    let trace = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let trace_lines = std::sync::Arc::clone(&trace);
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            trace_lines.lock().expect("trace lines").push(line);
        }
    });
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
    send_message(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "processId": null,
                "rootUri": project_uri,
                "workspaceFolders": [{"uri": project_uri, "name": "EditPublish"}],
                "capabilities": {},
                "initializationOptions": {"modHost": {
                    "path": mod_host,
                    "runtimePrefix": runtime_prefix,
                    "runtimeProfile": "debug"
                }}
            }
        }),
    );
    assert!(response_with_id(&messages, 1).get("error").is_none());
    send_message(&mut stdin, json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    // The workspace index finishes first, so no compilation-context change meets the edit.
    loop {
        let message = messages.recv_timeout(Duration::from_secs(60)).expect("LSP status before timeout");
        if message.get("method").and_then(Value::as_str) == Some("beskid/status")
            && message["params"]["phase"].as_str() == Some("idle")
            && message["params"]["active"].as_bool() == Some(false)
        {
            break;
        }
    }
    send_message(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {"textDocument": {"uri": source_uri, "languageId": "beskid", "version": 1, "text": source}}
        }),
    );
    wait_for_document_diagnostics(&messages, &source_uri, 1);
    assert!(
        wait_for_trace(&trace, &format!(" publish uri={source_uri} version=1 ")),
        "opening a document runs the full preparation"
    );

    let broken = "i32 Main() {\n    return \"wrong\";\n}\n";
    send_message(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {"textDocument": {"uri": source_uri, "version": 2}, "contentChanges": [{"text": broken}]}
        }),
    );
    let mut published = Vec::new();
    while let Some(message) = next_document_diagnostics(&messages, &source_uri, Duration::from_secs(5)) {
        if message["params"]["version"].as_i64() == Some(2) {
            published.push(message);
        }
    }
    assert_eq!(published.len(), 1, "one publication for the edit: {published:#?}");
    assert!(
        published[0]["params"]["diagnostics"].as_array().is_some_and(|diagnostics| !diagnostics.is_empty()),
        "the edit's error is published: {:#}",
        published[0]
    );
    assert!(
        wait_for_trace(&trace, &format!("full-pass-skipped uri={source_uri} version=2")),
        "the edit's analysis made the full preparation redundant"
    );
    assert!(
        !trace
            .lock()
            .expect("trace lines")
            .iter()
            .any(|line| line.contains(&format!(" publish uri={source_uri} version=2 "))),
        "no full preparation ran for the edit"
    );

    send_message(&mut stdin, json!({"jsonrpc": "2.0", "id": 9, "method": "shutdown", "params": null}));
    let _ = response_with_id(&messages, 9);
    send_message(&mut stdin, json!({"jsonrpc": "2.0", "method": "exit", "params": null}));
    drop(stdin);
    assert!(child.wait().expect("wait for server").success());
}
