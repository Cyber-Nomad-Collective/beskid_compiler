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
        let message = messages.recv_timeout(Duration::from_secs(30)).expect("LSP response before timeout");
        if message.get("id").and_then(Value::as_i64) == Some(id) {
            return message;
        }
    }
}

fn wait_for_idle_status(messages: &Receiver<Value>) {
    loop {
        let message = messages.recv_timeout(Duration::from_secs(30)).expect("LSP notification before timeout");
        if message.get("method").and_then(Value::as_str) == Some("beskid/status")
            && message["params"]["phase"].as_str() == Some("idle")
            && message["params"]["active"].as_bool() == Some(false)
        {
            return;
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
    let compiler_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("compiler root");
    let project = tempfile::tempdir_in(compiler_root.join("target")).expect("temporary project");
    std::fs::create_dir(project.path().join("Src")).expect("source directory");
    std::fs::write(
        project.path().join("ProtocolSmoke.bproj"),
        "ProtocolSmoke {\n  name = \"ProtocolSmoke\"\n  version = \"0.1.0\"\n}\n\ntarget \"App\" {\n  kind = App\n  entry = \"Main.bd\"\n}\n",
    )
    .expect("project manifest");
    let source = "use Std.Core.Output;\n\ni32 Main() {\n    Output.WriteLine(\"ok\");\n    return 0;\n}\n";
    let source_path = project.path().join("Src/Main.bd");
    std::fs::write(&source_path, source).expect("source file");

    let project_uri = url::Url::from_directory_path(project.path()).expect("project URI").to_string();
    let source_uri = url::Url::from_file_path(&source_path).expect("source URI").to_string();
    let mut child = Command::new(env!("CARGO_BIN_EXE_beskid_lsp"))
        .current_dir(compiler_root)
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
                "capabilities": {}
            }
        }),
    );
    assert!(response_with_id(&messages, 1).get("error").is_none());
    send_message(&mut stdin, json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    wait_for_idle_status(&messages);
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
    let labels = completion["result"]
        .as_array()
        .expect("completion array")
        .iter()
        .filter_map(|item| item["label"].as_str())
        .collect::<Vec<_>>();
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

    send_message(&mut stdin, json!({"jsonrpc": "2.0", "id": 4, "method": "shutdown", "params": null}));
    let _ = response_with_id(&messages, 4);
    send_message(&mut stdin, json!({"jsonrpc": "2.0", "method": "exit", "params": null}));
    drop(stdin);
    assert!(child.wait().expect("wait for server").success());
}
