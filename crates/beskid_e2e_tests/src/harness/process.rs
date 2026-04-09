use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub fn run_binary(path: &Path, timeout: Duration) -> Output {
    let mut child = Command::new(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("spawn binary {}: {error}", path.display()));

    let start = Instant::now();
    loop {
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            panic!("timed out running binary {}", path.display());
        }

        match child.try_wait().expect("wait on child process") {
            Some(_) => return child.wait_with_output().expect("collect child output"),
            None => thread::sleep(Duration::from_millis(20)),
        }
    }
}

pub fn nm_contains_symbol(path: &Path, symbol: &str) -> bool {
    let output = Command::new("nm")
        .arg(path)
        .output()
        .unwrap_or_else(|error| panic!("run nm on {}: {error}", path.display()));
    assert!(output.status.success(), "nm failed for {}", path.display());
    String::from_utf8_lossy(&output.stdout).contains(symbol)
}
