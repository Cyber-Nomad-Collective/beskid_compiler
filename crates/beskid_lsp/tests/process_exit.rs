//! The server process ends when its client goes away: `exit` without waiting for in-flight
//! work, the end of stdin, the death of the client process named in `initialize`.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tempfile::TempDir;

/// Longest the process may take to end after the event that must end it.
const EXIT_WITHIN: Duration = Duration::from_secs(5);

fn send_message(stdin: &mut ChildStdin, message: Value) {
    let body = serde_json::to_vec(&message).expect("serialize message");
    write!(stdin, "Content-Length: {}\r\n\r\n", body.len()).expect("write header");
    stdin.write_all(&body).expect("write body");
    stdin.flush().expect("flush");
}

/// A workspace with one small project, so the initial index has work to do.
fn workspace() -> TempDir {
    let temp = TempDir::new().expect("workspace");
    std::fs::create_dir_all(temp.path().join("Src")).expect("src");
    std::fs::write(
        temp.path().join("App.bproj"),
        "App {\n  name = \"App\"\n  version = \"0.1.0\"\n}\n\ntarget \"App\" {\n  kind = App\n  entry = \"Main.bd\"\n}\n",
    )
    .expect("manifest");
    std::fs::write(temp.path().join("Src/Main.bd"), "i32 Main() { return 0; }\n").expect("source");
    temp
}

struct Server {
    child: Child,
    stdin: Option<ChildStdin>,
    messages: Receiver<Value>,
    _home: TempDir,
}

/// Start a server on `root`, initialized with `process_id` as its client, with the index of
/// `root` under way.
fn start(root: &std::path::Path, process_id: Value) -> Server {
    let home = TempDir::new().expect("home");
    let mut child = Command::new(env!("CARGO_BIN_EXE_beskid_lsp"))
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .env("BESKID_HOME", home.path().join("beskid"))
        .env_remove("BESKID_CORELIB_ROOT")
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
            let message: Value = serde_json::from_slice(&body).ok()?;
            sender.send(message).ok()?;
        }
    });
    let root_uri = url::Url::from_directory_path(root).expect("root URI").to_string();
    send_message(
        &mut stdin,
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "processId": process_id, "rootUri": root_uri, "capabilities": {},
            "workspaceFolders": [{"uri": root_uri, "name": "ws"}]
        }}),
    );
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let message =
            messages.recv_timeout(deadline.saturating_duration_since(Instant::now())).expect("initialize answer");
        if message.get("id") == Some(&json!(1)) {
            break;
        }
    }
    send_message(&mut stdin, json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    Server { child, stdin: Some(stdin), messages, _home: home }
}

fn exit_status_within(child: &mut Child, limit: Duration) -> Option<std::process::ExitStatus> {
    let deadline = Instant::now() + limit;
    loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            return Some(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn stdin_closed_without_shutdown_ends_the_process() {
    let root = workspace();
    let mut server = start(root.path(), json!(std::process::id()));
    drop(server.stdin.take());
    let status = exit_status_within(&mut server.child, EXIT_WITHIN).expect("the server ends when its stdin ends");
    assert!(!status.success(), "the client went away without `exit`: {status}");
}

#[test]
fn exit_during_the_initial_index_ends_the_process_promptly() {
    let root = workspace();
    let mut server = start(root.path(), json!(std::process::id()));
    let stdin = server.stdin.as_mut().expect("stdin");
    send_message(stdin, json!({"jsonrpc": "2.0", "method": "exit", "params": null}));
    // stdin stays open: `exit` alone must end the process.
    let status = exit_status_within(&mut server.child, EXIT_WITHIN).expect("the server ends on `exit`");
    assert_eq!(status.code(), Some(1), "`exit` without `shutdown` ends with code 1: {status}");
    drop(server.messages);
}

#[test]
fn shutdown_then_exit_ends_the_process_with_success() {
    let root = workspace();
    let mut server = start(root.path(), json!(std::process::id()));
    let stdin = server.stdin.as_mut().expect("stdin");
    send_message(stdin, json!({"jsonrpc": "2.0", "id": 2, "method": "shutdown", "params": null}));
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let message =
            server.messages.recv_timeout(deadline.saturating_duration_since(Instant::now())).expect("shutdown answer");
        if message.get("id") == Some(&json!(2)) {
            break;
        }
    }
    send_message(stdin, json!({"jsonrpc": "2.0", "method": "exit", "params": null}));
    let status = exit_status_within(&mut server.child, EXIT_WITHIN).expect("the server ends on `exit`");
    assert!(status.success(), "{status}");
}

#[cfg(unix)]
#[test]
fn the_death_of_the_client_process_ends_the_server() {
    // A second server instance stands in for the client: alive while its stdin is open.
    let client_root = workspace();
    let mut client = Command::new(env!("CARGO_BIN_EXE_beskid_lsp"))
        .env("HOME", client_root.path())
        .env("BESKID_HOME", client_root.path().join("beskid"))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the stand-in client");
    let root = workspace();
    let mut server = start(root.path(), json!(client.id()));
    assert!(server.child.try_wait().expect("try_wait").is_none(), "the server runs while its client lives");
    client.kill().expect("kill the client");
    client.wait().expect("reap the client");
    let status = exit_status_within(&mut server.child, EXIT_WITHIN + Duration::from_secs(2))
        .expect("the server ends once its client process is gone");
    assert!(!status.success(), "{status}");
}
