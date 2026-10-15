//! Real terminal and pipe contracts for line-oriented REPL and template prompts.

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
import errno, fcntl, json, os, pty, re, select, signal, subprocess, sys, tempfile, termios, time
binary, command, mode = sys.argv[1:]
with tempfile.TemporaryDirectory(prefix='beskid-line-interaction-') as root:
    terminal = mode.startswith('pty')
    args = [binary] + {'repl': ['dev', 'repl'], 'graph': ['dev', 'project', 'graph']}.get(command, [command])
    if command == 'repl':
        payload = b':help\n2 + 3\n' + (b':quit\n' if mode.endswith('quit') else b'')
    elif command == 'new':
        template = os.path.join(root, 'template')
        os.makedirs(os.path.join(template, '.beskid'))
        with open(os.path.join(template, '.beskid', 'template.json'), 'w') as out:
            json.dump({'schema': 'beskid.template.v1', 'identity': 'line-fixture', 'name': 'Line fixture', 'shortName': 'line-fixture',
                       'tags': {'type': 'project'}, 'sources': [{}], 'symbols': {
                           'flavor': {'type': 'choice', 'isRequired': True, 'choices': ['small', 'large']},
                           'name': {'type': 'string', 'isRequired': True}}, 'postActions': []}, out)
        with open(os.path.join(template, '{{name}}.txt'), 'w') as out:
            out.write('{{name}}:{{flavor}}\n')
        with open(os.path.join(template, '{{name}}.bproj'), 'w') as out:
            out.write('{{name}} { name = "{{name}}" version = "0.1.0" }\ntarget "{{name}}" { kind = "App" entry = "Smoke.bd" }\n')
        os.mkdir(os.path.join(template, 'Src'))
        with open(os.path.join(template, 'Src', 'Smoke.bd'), 'w') as out:
            out.write('pub i64 Main() { return 0; }\n')
        args += ['LineExample', '--path', template, '-o', os.path.join(root, 'output')]
        payload = b'2\n'
    else:
        os.mkdir(os.path.join(root, 'Src'))
        with open(os.path.join(root, 'Src', 'Smoke.bd'), 'w') as out:
            out.write('pub i64 Main() { return 0; }\n')
        with open(os.path.join(root, 'Smoke.bproj'), 'w') as out:
            out.write('Smoke { name = "Smoke" version = "0.1.0" }\ntarget "Smoke" { kind = "App" entry = "Smoke.bd" }\n')
        args += ['--project', root, '--plain']
        args += ['--out', os.path.join(root, 'graph.mmd')] if mode == 'file' else ['--' + mode]
        payload = b''
    if mode == 'pty-plain-quit':
        args += ['--plain']
    env = dict(os.environ, TERM='xterm-256color', OTEL_SDK_DISABLED='true', BESKID_CORELIB_ROOT=os.path.join(root, 'corelib'))
    env.pop('RUST_LOG', None)
    master = slave = None
    def attach_terminal():
        os.setsid()
        fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
    if terminal:
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, b'\x18\x00\x50\x00\x00\x00\x00\x00')
        process = subprocess.Popen(args, stdin=slave, stdout=slave, stderr=slave, preexec_fn=attach_terminal, cwd=root, env=env)
    else:
        process = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, start_new_session=True, cwd=root, env=env)
        process.stdin.write(payload)
        process.stdin.close()
    stream = master if terminal else process.stdout.fileno()
    data = bytearray()
    raw_seen = False
    sent_lines = 0
    input_lines = payload.splitlines(keepends=True)
    eof_sent = False
    deadline = time.monotonic() + 15
    try:
        while process.poll() is None and time.monotonic() < deadline:
            if terminal:
                flags = termios.tcgetattr(slave)[3]
                raw_seen |= not bool(flags & termios.ICANON) or not bool(flags & termios.ECHO)
                ready = (data.count(b'beskid> ') > sent_lines) if command == 'repl' else (b'> ' in data)
                if ready and sent_lines < len(input_lines):
                    os.write(master, input_lines[sent_lines])
                    sent_lines += 1
                elif ready and mode.endswith('eof') and not eof_sent:
                    os.write(master, b'\x04')
                    eof_sent = True
            if select.select([stream], [], [], 0.01)[0]:
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
        assert not raw_seen, 'line command enabled raw input: ' + repr(text[-2000:])
        assert not timed_out, 'line command did not exit after :quit/EOF: ' + repr(text[-2000:])
        assert process.returncode == 0, 'command failed: ' + text
        if command != 'graph':
            # New's default lock post-action may use the approved bounded progress line.
            line_data = data.replace(b'\x1b[2K', b'') if command == 'new' else data
            assert not any(byte < 32 and byte not in (10, 13) for byte in line_data), 'control bytes in line output: ' + repr(text[-2000:])
            if command == 'new':
                bars = [line for line in re.findall(rb'\x1b\[2K([^\r\n\x1b]*)', data) if line.startswith(b'[')]
                assert all(len(line) <= 79 for line in bars), 'template post-action progress exceeds terminal width: ' + repr(bars)
        if command == 'repl':
            assert 'commands: :quit, :reset, :type <snippet>' in text, 'line command result missing: ' + text
            assert '5' in text.replace('\r', '').splitlines(), 'scalar line result missing: ' + text
            if terminal:
                assert b'beskid> ' in data
        elif command == 'new':
            assert '1. small' in text and '2. large' in text, 'required prompts missing: ' + text
            assert text.count('flavor [flavor]') == 1 and 'name [name]' not in text, 'positional name must be bound without a prompt: ' + text
            with open(os.path.join(root, 'output', 'LineExample.txt')) as out:
                assert out.read() == 'LineExample:large\n'
        elif mode == 'file':
            with open(os.path.join(root, 'graph.mmd')) as out:
                assert 'flowchart' in out.read()
        elif mode == 'mermaid':
            assert 'flowchart' in text
        else:
            assert 'Smoke' in text and 'flowchart' not in text, 'graph TUI renderer missing: ' + text
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
macro_rules! cases {
    ($($name:ident: $command:literal, $mode:literal;)*) => {$ (
        #[test]
        fn $name() { exercise($command, $mode); }
    )*};
}

#[cfg(target_os = "linux")]
cases! {
    repl_pipe_quit: "repl", "pipe-quit";
    repl_pipe_eof: "repl", "pipe-eof";
    repl_terminal_quit: "repl", "pty-quit";
    repl_terminal_eof: "repl", "pty-eof";
    repl_plain_terminal_quit: "repl", "pty-plain-quit";
    new_terminal_required_values_and_choice: "new", "pty";
    graph_mermaid: "graph", "mermaid";
    graph_file_output: "graph", "file";
    graph_forced_tui: "graph", "tui";
}
