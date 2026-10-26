//! Release gates for a many-project folder: member completion answers quickly while the
//! workspace scan is still running, and edits that alternate between projects keep hover,
//! completion and the outline within their latency budgets.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
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

/// Wait for the diagnostics of the opened buffer (version 1), not those the workspace scan
/// publishes for the file on disk.
fn wait_for_document_diagnostics(messages: &Receiver<Value>, uri: &str, scan_idle: &mut bool) {
    loop {
        let message = messages.recv_timeout(Duration::from_secs(120)).expect("diagnostics before timeout");
        if is_idle(&message) {
            *scan_idle = true;
        }
        if message.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics")
            && message["params"]["uri"].as_str() == Some(uri)
            && message["params"]["version"].as_i64() == Some(1)
        {
            return;
        }
    }
}

fn labels(response: &Value) -> Vec<&str> {
    response["result"].as_array().expect("completion array").iter().filter_map(|item| item["label"].as_str()).collect()
}

/// A folder of [`PROJECTS`] small App projects under the compiler's `target` directory.
fn write_projects() -> (tempfile::TempDir, &'static Path) {
    let compiler_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().and_then(Path::parent).expect("compiler root");
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
    (folder, compiler_root)
}

/// Start the server on `folder` (initialized, Mod host from the workspace) and return it with
/// its stdin and a channel of every message it sends.
fn start_server(folder: &Path, compiler_root: &Path, idle: &mut bool) -> (Child, ChildStdin, Receiver<Value>) {
    let folder_uri = url::Url::from_directory_path(folder).expect("folder URI").to_string();
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
    send_message(
        &mut stdin,
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "processId": null, "rootUri": folder_uri,
            "workspaceFolders": [{"uri": folder_uri, "name": "many"}],
            "capabilities": {},
            "initializationOptions": {"modHost": {
                "path": mod_host, "runtimePrefix": runtime_prefix, "runtimeProfile": "debug"}}}}),
    );
    assert!(response_with_id(&messages, 1, idle).get("error").is_none());
    send_message(&mut stdin, json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    (child, stdin, messages)
}

#[test]
fn member_completion_answers_within_a_second_while_a_multi_project_scan_runs() {
    let (folder, compiler_root) = write_projects();
    let source = "use Core.String;\n\ni32 Main() {\n    String.Sea;\n    return 0;\n}\n";
    let source_path = folder.path().join("P00/Src/Main.bd");
    std::fs::write(&source_path, source).expect("target source");
    let source_uri = url::Url::from_file_path(&source_path).expect("source URI").to_string();
    let mut idle = false;
    let (mut child, mut stdin, messages) = start_server(folder.path(), compiler_root, &mut idle);
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

/// Edits that alternate between documents of several projects. Each project keeps its analysis
/// between edits, so the outline (syntax only) answers at once and hover and completion wait only
/// for the edit's own fast pass, never for a project to be analyzed again from the start.
#[test]
fn edits_alternating_between_projects_keep_requests_within_budget() {
    const EDITED: [&str; 4] = ["P10", "P11", "P12", "P13"];
    const ROUNDS: usize = 16;
    // Debug builds on a loaded builder: generous, but far below a re-analysis of the project.
    const OUTLINE_LIMIT: Duration = Duration::from_millis(500);
    const REQUEST_LIMIT: Duration = Duration::from_secs(5);
    const REQUEST_MEDIAN_LIMIT: Duration = Duration::from_secs(1);

    let (folder, compiler_root) = write_projects();
    let base = "use Core.String;\n\ni32 Main() {\n    String.Sea;\n    return 0;\n}\n";
    let mut idle = false;
    let (mut child, mut stdin, messages) = start_server(folder.path(), compiler_root, &mut idle);
    let uris: Vec<String> = EDITED
        .iter()
        .map(|project| {
            let path = folder.path().join(format!("{project}/Src/Main.bd"));
            std::fs::write(&path, base).expect("edited source");
            url::Url::from_file_path(&path).expect("edited URI").to_string()
        })
        .collect();
    for uri in &uris {
        send_message(
            &mut stdin,
            json!({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {"textDocument": {
                "uri": uri, "languageId": "beskid", "version": 1, "text": base}}}),
        );
        wait_for_document_diagnostics(&messages, uri, &mut idle);
    }

    let mut next_id = 100;
    let mut request = |stdin: &mut ChildStdin, method: &str, params: Value, idle: &mut bool| {
        next_id += 1;
        let asked = Instant::now();
        send_message(stdin, json!({"jsonrpc": "2.0", "id": next_id, "method": method, "params": params}));
        let response = response_with_id(&messages, next_id, idle);
        (response, asked.elapsed())
    };
    let mut hovers = Vec::new();
    let mut completions = Vec::new();
    for round in 0..ROUNDS {
        let uri = &uris[round % EDITED.len()];
        let text = format!("{base}// edit {round}\n");
        send_message(
            &mut stdin,
            json!({"jsonrpc": "2.0", "method": "textDocument/didChange", "params": {
                "textDocument": {"uri": uri, "version": round + 2},
                "contentChanges": [{"text": text}]}}),
        );
        let (outline, outline_latency) =
            request(&mut stdin, "textDocument/documentSymbol", json!({"textDocument": {"uri": uri}}), &mut idle);
        assert!(
            outline["result"].as_array().is_some_and(|symbols| symbols.iter().any(|s| s["name"] == "Main")),
            "outline of round {round}: {outline}"
        );
        assert!(outline_latency < OUTLINE_LIMIT, "outline of round {round} took {outline_latency:?}");
        let (hover, hover_latency) = request(
            &mut stdin,
            "textDocument/hover",
            json!({"textDocument": {"uri": uri}, "position": {"line": 3, "character": 6}}),
            &mut idle,
        );
        assert!(hover.get("error").is_none(), "hover of round {round}: {hover}");
        let (completion, completion_latency) = request(
            &mut stdin,
            "textDocument/completion",
            json!({"textDocument": {"uri": uri}, "position": {"line": 3, "character": 14}}),
            &mut idle,
        );
        let got = labels(&completion);
        assert!(got.contains(&"Search"), "completion of round {round}: {got:?}");
        eprintln!(
            "round {round}: outline {outline_latency:?}, hover {hover_latency:?}, completion {completion_latency:?}"
        );
        hovers.push(hover_latency);
        completions.push(completion_latency);
    }
    for (name, mut latencies) in [("hover", hovers), ("completion", completions)] {
        latencies.sort();
        let median = latencies[latencies.len() / 2];
        let max = *latencies.last().expect("rounds");
        assert!(max < REQUEST_LIMIT, "{name} took {max:?} after an edit in another project");
        assert!(median < REQUEST_MEDIAN_LIMIT, "{name} median {median:?} over {REQUEST_MEDIAN_LIMIT:?}");
    }
    let _ = child.kill();
}
