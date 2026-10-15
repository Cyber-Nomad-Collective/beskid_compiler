//! Deterministic child process for the 0.6 release process cases (R4-PROC-01..06).
//!
//! The release producer passes this executable to the native `ProcessSuite` tests as
//! `BESKID_V06_RELEASE_FIXTURE`; its digest is the `artifact_sha256` of every R4-PROC evidence
//! row. The mode is the first argument:
//!
//! - `echo-argv-json <args...>`: print the arguments after the mode as one compact JSON array of
//!   strings and a newline (non-ASCII text verbatim; `"`, `\` and control characters escaped).
//! - `cat-binary [--chunk <n>]`: copy stdin to stdout in writes of at most `n` bytes (default
//!   65536), flushing each write, until end of input.
//! - `flood-stderr <bytes>`: write exactly that many ASCII bytes to stderr, nothing to stdout.
//! - `exit <code>`: exit with the code.
//! - `sleep-forever`: block until killed.
//! - `grandchild-inherits-pipes`: start `sleep-forever` as a grandchild that inherits this
//!   process's standard streams, then block until killed.
//!
//! An unknown mode or a malformed argument exits with status 64 and a message on stderr.
//!
//! Copied under the file stem `beskid-hanging-tool`, the fixture plays an external tool whose
//! probe never answers (R4-TOOL-04): it ignores its arguments, writes its process id to
//! `beskid-hanging-tool.pid` beside itself and blocks until killed.
use std::io::{Read, Write};
use std::process::{Command, ExitCode, Stdio};

fn usage(message: &str) -> ExitCode {
    eprintln!("beskid_release_fixture: {message}");
    ExitCode::from(64)
}

fn json_string(text: &str, output: &mut String) {
    output.push('"');
    for character in text.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            '\u{8}' => output.push_str("\\b"),
            '\u{c}' => output.push_str("\\f"),
            control if (control as u32) < 0x20 => output.push_str(&format!("\\u{:04x}", control as u32)),
            other => output.push(other),
        }
    }
    output.push('"');
}

fn echo_argv_json(arguments: &[String]) -> ExitCode {
    let mut text = String::from("[");
    for (index, argument) in arguments.iter().enumerate() {
        if index > 0 {
            text.push(',');
        }
        json_string(argument, &mut text);
    }
    text.push_str("]\n");
    let mut stdout = std::io::stdout().lock();
    if stdout.write_all(text.as_bytes()).and_then(|()| stdout.flush()).is_err() {
        return ExitCode::from(74);
    }
    ExitCode::SUCCESS
}

fn cat_binary(arguments: &[String]) -> ExitCode {
    let chunk = match arguments {
        [] => 65536,
        [flag, value] if flag == "--chunk" => match value.parse::<usize>() {
            Ok(chunk) if chunk > 0 => chunk,
            _ => return usage("--chunk needs a positive byte count"),
        },
        _ => return usage("cat-binary accepts only `--chunk <n>`"),
    };
    let mut stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    let mut buffer = vec![0u8; chunk];
    loop {
        let read = match stdin.read(&mut buffer) {
            Ok(0) => return ExitCode::SUCCESS,
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return ExitCode::from(74),
        };
        if stdout.write_all(&buffer[..read]).and_then(|()| stdout.flush()).is_err() {
            return ExitCode::from(74);
        }
    }
}

fn flood_stderr(arguments: &[String]) -> ExitCode {
    let [count] = arguments else { return usage("flood-stderr needs a byte count") };
    let Ok(mut remaining) = count.parse::<usize>() else { return usage("flood-stderr needs a byte count") };
    let block = [b'e'; 8192];
    let mut stderr = std::io::stderr().lock();
    while remaining > 0 {
        let length = remaining.min(block.len());
        if stderr.write_all(&block[..length]).is_err() {
            return ExitCode::from(74);
        }
        remaining -= length;
    }
    if stderr.flush().is_err() {
        return ExitCode::from(74);
    }
    ExitCode::SUCCESS
}

fn exit_with(arguments: &[String]) -> ExitCode {
    let [code] = arguments else { return usage("exit needs a status") };
    match code.parse::<u8>() {
        Ok(code) => ExitCode::from(code),
        Err(_) => usage("exit status must be 0..=255"),
    }
}

fn sleep_forever() -> ! {
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}

fn grandchild_inherits_pipes() -> ExitCode {
    let Ok(executable) = std::env::current_exe() else { return usage("current executable is unavailable") };
    let spawned = Command::new(executable)
        .arg("sleep-forever")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn();
    match spawned {
        Ok(_grandchild) => sleep_forever(),
        Err(error) => usage(&format!("grandchild did not start: {error}")),
    }
}

/// Stem under which the fixture plays a tool whose probe never answers.
const HANGING_TOOL_STEM: &str = "beskid-hanging-tool";

fn hanging_tool(executable: &std::path::Path) -> ExitCode {
    let marker = executable.with_file_name(format!("{HANGING_TOOL_STEM}.pid"));
    if std::fs::write(&marker, std::process::id().to_string()).is_err() {
        return ExitCode::from(74);
    }
    sleep_forever()
}

fn main() -> ExitCode {
    if let Ok(executable) = std::env::current_exe()
        && executable.file_stem().and_then(|stem| stem.to_str()) == Some(HANGING_TOOL_STEM)
    {
        return hanging_tool(&executable);
    }
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let Some((mode, rest)) = arguments.split_first() else { return usage("missing mode") };
    match mode.as_str() {
        "echo-argv-json" => echo_argv_json(rest),
        "cat-binary" => cat_binary(rest),
        "flood-stderr" => flood_stderr(rest),
        "exit" => exit_with(rest),
        "sleep-forever" if rest.is_empty() => sleep_forever(),
        "grandchild-inherits-pipes" if rest.is_empty() => grandchild_inherits_pipes(),
        other => usage(&format!("unknown mode `{other}`")),
    }
}
