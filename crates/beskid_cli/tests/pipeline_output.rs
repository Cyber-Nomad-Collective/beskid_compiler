//! Exercise command completion with pipes and a real controlling terminal.
//! Linux's PTY runner also samples termios: raw input cannot be inferred from output bytes alone.

#[cfg(target_os = "linux")]
fn exercise(command: &str, mode: &str) {
    let output = std::process::Command::new("python3")
        .args(["-c", RUNNER, env!("CARGO_BIN_EXE_beskid_cli"), command, mode])
        .output()
        .expect("Python PTY runner");
    assert!(
        output.status.success(),
        "{command}/{mode}: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(target_os = "linux")]
const RUNNER: &str = r#"
import errno, fcntl, os, pty, re, select, signal, subprocess, sys, tempfile, termios, time
binary, command, mode = sys.argv[1:]
with tempfile.TemporaryDirectory(prefix='beskid-pipeline-output-') as root:
    os.mkdir(os.path.join(root, 'Src'))
    source = os.path.join(root, 'Src', 'Smoke.bd')
    with open(source, 'w') as out:
        code = '7' if 'exit7' in mode else '0'
        out.write('pub i64 Main() { return ' + code + '; }\ntest Smoke { }\n')
    with open(os.path.join(root, 'Smoke.bproj'), 'w') as out:
        kind = 'Test' if command == 'test' else 'App'
        out.write('Smoke { name = "Smoke" version = "0.1.0" }\n'
                  'target "Smoke" { kind = "' + kind + '" entry = "Smoke.bd" }\n')
    args = [binary, command, '--project', root]
    if command == 'build':
        args += ['--kind', 'object', '--output', os.path.join(root, 'Smoke.o')]
    if mode.endswith('plain'):
        args += ['--plain']
    env = dict(os.environ, TERM='xterm-256color')
    env.pop('RUST_LOG', None)
    env.pop('NO_COLOR', None)
    env.pop('CI', None)
    env['BESKID_CORELIB_ROOT'] = os.path.join(root, 'corelib')
    terminal = mode.startswith('pty')
    master = slave = None
    def attach_terminal():
        os.setsid()
        fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
    if terminal:
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, b'\x18\x00\x50\x00\x00\x00\x00\x00')
        process = subprocess.Popen(args, stdin=slave, stdout=slave, stderr=slave,
                                   preexec_fn=attach_terminal, cwd=root, env=env)
    else:
        process = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                   stderr=subprocess.STDOUT, start_new_session=True, cwd=root, env=env)
    data = bytearray()
    raw_seen = False
    deadline = time.monotonic() + 30
    stream = master if terminal else process.stdout.fileno()
    try:
        while process.poll() is None and time.monotonic() < deadline:
            if terminal:
                flags = termios.tcgetattr(slave)[3]
                raw_seen |= not bool(flags & termios.ICANON) or not bool(flags & termios.ECHO)
            ready, _, _ = select.select([stream], [], [], 0.01)
            if ready:
                data.extend(os.read(stream, 65536))
        timed_out = process.poll() is None
        if timed_out:
            os.killpg(process.pid, signal.SIGKILL)
        process.wait()
        if terminal:
            os.close(slave)
            slave = None
        while select.select([stream], [], [], 0.05)[0]:
            try:
                chunk = os.read(stream, 65536)
            except OSError as error:
                if error.errno == errno.EIO:
                    break
                raise
            if not chunk:
                break
            data.extend(chunk)
        text = data.decode('utf-8', errors='replace')
        assert not timed_out, 'command waited for terminal input after work: ' + repr(text[-2000:])
        expected_exit = 7 if 'exit7' in mode else 0
        assert process.returncode == expected_exit, 'wrong command exit status: ' + text
        assert not raw_seen, 'ordinary command enabled raw terminal input: ' + repr(text[-2000:])
        for code in (b'\x1b[?1049', b'\x1b[?1000', b'\x1b[?1002', b'\x1b[?1003', b'\x1b[?1006'):
            assert code not in data, 'ordinary command entered terminal UI: ' + repr(text[-2000:])
        if terminal and not mode.endswith('plain'):
            bars = [line for line in re.findall(rb'\x1b\[2K([^\r\n\x1b]*)', data) if line.startswith(b'[')]
            assert bars, 'TTY progress bar missing: ' + text
            assert all(len(line) <= 79 for line in bars), 'progress exceeds terminal width: ' + repr(bars)
            assert all(b' INFO ' not in line for line in bars), 'tracing appended to progress line: ' + repr(bars)
        if not terminal or mode.endswith('plain'):
            assert not any(byte < 32 and byte not in (10, 13) for byte in data), 'control bytes in plain/pipe output: ' + repr(text)
        assert 'Resolve' in text, 'resolve phase tree missing: ' + text
        assert ('|- ' if not terminal or mode.endswith('plain') else '├─ ') in text, 'nested phase tree missing: ' + text
        summary = {'build': 'Build complete', 'run': 'Run complete', 'analyze': 'Analyze complete', 'test': 'Result: passed=1, failed=0'}[command]
        assert summary in text, 'completion summary missing: ' + text
        if command == 'build':
            assert 'output:' in text, 'build output path missing from final summary: ' + text
            assert os.path.getsize(os.path.join(root, 'Smoke.o')) > 0
        if command == 'run':
            assert 'exit: ' + str(expected_exit) in text, 'child exit status missing from final summary: ' + text
    finally:
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
        if slave is not None:
            os.close(slave)
        if master is not None:
            os.close(master)
"#;

#[cfg(target_os = "linux")]
macro_rules! output_cases {
    ($($name:ident: $command:literal, $mode:literal;)*) => {$ (
        #[test]
        fn $name() { exercise($command, $mode); }
    )*};
}

#[cfg(target_os = "linux")]
output_cases! {
    build_pipe_completion: "build", "pipe";
    analyze_pipe_completion: "analyze", "pipe";
    test_pipe_completion: "test", "pipe";
    build_plain_pipe_completion: "build", "pipe-plain";
    analyze_plain_pipe_completion: "analyze", "pipe-plain";
    test_plain_pipe_completion: "test", "pipe-plain";
    build_pty_completion_without_input: "build", "pty";
    analyze_pty_completion_without_input: "analyze", "pty";
    test_pty_completion_without_input: "test", "pty";
    build_plain_pty_completion_without_input: "build", "pty-plain";
    analyze_plain_pty_completion_without_input: "analyze", "pty-plain";
    test_plain_pty_completion_without_input: "test", "pty-plain";
    run_pipe_completion: "run", "pipe";
    run_plain_pipe_completion: "run", "pipe-plain";
    run_pty_completion_without_input: "run", "pty";
    run_plain_pty_completion_without_input: "run", "pty-plain";
    run_plain_pipe_preserves_child_exit_status: "run", "pipe-exit7-plain";
}
