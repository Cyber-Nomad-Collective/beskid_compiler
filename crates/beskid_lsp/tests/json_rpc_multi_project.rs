//! Release gate: member completion in a many-project folder answers quickly while the
//! workspace scan is still running.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const PROJECTS: usize = 30;

fn send_message(stdin: &mut ChildStdin, message: Value) {
    let body = serde_json::to_vec(&message).expect("serialize LSP message");
    write!(stdin, "Content-Length: {}\r\n\r\n", body.len()).expect("write LSP header");
    stdin.write_all(&body).expect("write LSP body");
    stdin.flush().expect("flush LSP message");
}

fn is_idle(message: &Value) -> bool {
    message.get("method").and_then(Value::as_str) == Some("beskid/status")
        && message["params"]["phase"].as_str() == Some("idle")
        && message["params"]["active"].as_bool() == Some(false)
}

/// Wait for response `id`; reports whether the scan went idle before it arrived.
fn response_with_id(messages: &Receiver<Value>, id: i64, scan_idle: &mut bool) -> Value {
    loop {
        let message = messages
            .recv_timeout(Duration::from_secs(60))
            .unwrap_or_else(|error| panic!("LSP response {id} before timeout: {error}"));
        if is_idle(&message) {
            *scan_idle = true;
        }
        if message.get("id").and_then(Value::as_i64) == Some(id) {
            return message;
        }
    }
}

fn wait_for_document_diagnostics(messages: &Receiver<Value>, uri: &str, scan_idle: &mut bool) {
    loop {
        let message = messages.recv_timeout(Duration::from_secs(120)).expect("diagnostics before timeout");
        if is_idle(&message) {
            *scan_idle = true;
        }
        if message.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics")
            && message["params"]["uri"].as_str() == Some(uri)
        {
            return;
        }
    }
}

fn labels(response: &Value) -> Vec<&str> {
    response["result"].as_array().expect("completion array").iter().filter_map(|item| item["label"].as_str()).collect()
}

#[test]
fn member_completion_answers_within_a_second_while_a_multi_project_scan_runs() {
    let compiler_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("compiler root");
    let parent = compiler_root.join("target");
    std::fs::create_dir_all(&parent).expect("temporary parent");
    let folder = tempfile::tempdir_in(&parent).expect("workspace folder");
    for index in 0..PROJECTS {
        let name = format!("P{index:02}");
        let dir = folder.path().join(&name);
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
    let source = "use Core.String;\n\ni32 Main() {\n    String.Sea;\n    return 0;\n}\n";
    let source_path = folder.path().join("P00/Src/Main.bd");
    std::fs::write(&source_path, source).expect("target source");

    let folder_uri = url::Url::from_directory_path(folder.path()).expect("folder URI").to_string();
    let source_uri = url::Url::from_file_path(&source_path).expect("source URI").to_string();
    let mod_host = beskid_tests_mod_host::provisioned_cli().expect("workspace Mod host CLI");
    let runtime_prefix = beskid_tests_mod_host::shared_runtime_prefix().expect("shared runtime kit");
    let corelib_root = beskid_tests_mod_host::shared_corelib_root().expect("shared provisioned Corelib");
    let mut child = Command::new(env!("CARGO_BIN_EXE_beskid_lsp"))
        .current_dir(compiler_root)
        .env("BESKID_CORELIB_ROOT", &corelib_root)
        .env_remove("BESKID_RUNTIME_PREFIX")
        .env_remove("BESKID_RUNTIME_KIT_PROFILE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
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

    let mut idle = false;
    send_message(
        &mut stdin,
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "processId": null, "rootUri": folder_uri,
            "workspaceFolders": [{"uri": folder_uri, "name": "many"}],
            "capabilities": {},
            "initializationOptions": {"modHost": {
                "path": mod_host, "runtimePrefix": runtime_prefix, "runtimeProfile": "debug"}}}}),
    );
    assert!(response_with_id(&messages, 1, &mut idle).get("error").is_none());
    send_message(&mut stdin, json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    send_message(
        &mut stdin,
        json!({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {"textDocument": {
            "uri": source_uri, "languageId": "beskid", "version": 1, "text": source}}}),
    );
    let opened = Instant::now();
    wait_for_document_diagnostics(&messages, &source_uri, &mut idle);
    let facts_latency = opened.elapsed();

    let scan_was_running = !idle;
    let asked = Instant::now();
    send_message(
        &mut stdin,
        json!({"jsonrpc": "2.0", "id": 2, "method": "textDocument/completion", "params": {
            "textDocument": {"uri": source_uri}, "position": {"line": 3, "character": 14}}}),
    );
    let completion = response_with_id(&messages, 2, &mut idle);
    let latency = asked.elapsed();
    eprintln!(
        "multi-project completion: {PROJECTS} projects, open->diagnostics {facts_latency:?}, \
         completion latency {latency:?}, scan still running at request: {scan_was_running}"
    );
    let got = labels(&completion);
    assert!(got.contains(&"Search"), "`String.` members must complete after `use Core.String;`: {got:?}");
    assert!(latency < Duration::from_secs(1), "completion took {latency:?}");
    assert!(scan_was_running, "scan finished before the request; the gate was not exercised");

    // `use` paths: child modules after `Core.`, and the first segment, in other projects.
    for (index, (project, text, character, expected, id)) in [
        ("P01", "use Core.\n\ni32 Main() {\n    return 0;\n}\n", 9, "String", 3),
        ("P02", "use C\n\ni32 Main() {\n    return 0;\n}\n", 5, "Core", 4),
    ]
    .into_iter()
    .enumerate()
    {
        let path = folder.path().join(format!("{project}/Src/Main.bd"));
        let uri = url::Url::from_file_path(&path).expect("use-path URI").to_string();
        send_message(
            &mut stdin,
            json!({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {"textDocument": {
                "uri": uri, "languageId": "beskid", "version": 1, "text": text}}}),
        );
        wait_for_document_diagnostics(&messages, &uri, &mut idle);
        let running = !idle;
        let asked = Instant::now();
        send_message(
            &mut stdin,
            json!({"jsonrpc": "2.0", "id": id, "method": "textDocument/completion", "params": {
                "textDocument": {"uri": uri}, "position": {"line": 0, "character": character}}}),
        );
        let response = response_with_id(&messages, id, &mut idle);
        let latency = asked.elapsed();
        let got = labels(&response);
        eprintln!("use-path completion {index}: {latency:?}, scan running: {running}, {} items", got.len());
        assert!(got.contains(&expected), "`{text}` must offer {expected}: {got:?}");
        if index == 0 {
            assert!(got.contains(&"Results"), "`use Core.` must offer its child modules: {got:?}");
        }
        assert!(latency < Duration::from_secs(1), "use-path completion took {latency:?}");
    }

    // A member access being typed: `Module.` or `value.` ends its line above the next statement,
    // so the buffer does not parse with it. Completion still lists the module's public functions
    // and the value's fields and methods.
    for (project, text, line, character, expected, id) in [
        (
            "P03",
            "use Core.String;\n\ni32 Main() {\n    String.\n    return 0;\n}\n",
            3,
            11,
            &["Contains", "Split", "Trim"][..],
            5,
        ),
        (
            "P04",
            "type Point { i32 raw;\ni32 Sum(i32 first) { return first + raw; }\n}\n\ni32 Main() {\n    Point point = Point { raw: 1 };\n    point.\n    return 0;\n}\n",
            6,
            10,
            &["Sum", "raw"][..],
            6,
        ),
    ] {
        let path = folder.path().join(format!("{project}/Src/Main.bd"));
        let uri = url::Url::from_file_path(&path).expect("member-access URI").to_string();
        send_message(
            &mut stdin,
            json!({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {"textDocument": {
                "uri": uri, "languageId": "beskid", "version": 1, "text": text}}}),
        );
        wait_for_document_diagnostics(&messages, &uri, &mut idle);
        let running = !idle;
        let asked = Instant::now();
        send_message(
            &mut stdin,
            json!({"jsonrpc": "2.0", "id": id, "method": "textDocument/completion", "params": {
                "textDocument": {"uri": uri}, "position": {"line": line, "character": character}}}),
        );
        let response = response_with_id(&messages, id, &mut idle);
        let latency = asked.elapsed();
        let got = labels(&response);
        eprintln!("incomplete member completion {project}: {latency:?}, scan running: {running}, {} items", got.len());
        for label in expected {
            assert!(got.contains(label), "`{text}` must offer {label}: {got:?}");
        }
        assert!(latency < Duration::from_secs(1), "incomplete member completion took {latency:?}");
    }
    let _ = child.kill();
}
