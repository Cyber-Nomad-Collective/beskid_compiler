//! The installed candidate CLI and the private environment every invocation runs in.
use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Names the complete installed candidate prefix (`<prefix>/bin/beskid`, `<prefix>/lib/beskid-runtime`,
/// `<prefix>/beskid_corelib`). Absence is a test failure, never a skip.
pub const QUALIFIED_PREFIX_ENV: &str = "BESKID_V06_QUALIFIED_PREFIX";
/// Developer overrides that would let the installed CLI reach a source checkout or a development kit.
pub const FORBIDDEN_OVERRIDES: [&str; 3] = ["BESKID_CORELIB_ROOT", "BESKID_RUNTIME_PREFIX", "BESKID_RUNTIME_KIT_PROFILE"];
/// Upper bound for one installed invocation; the producer applies its own cell budget on top.
pub const INVOCATION_LIMIT: Duration = Duration::from_secs(600);

static QUALIFIED: OnceLock<Cli> = OnceLock::new();

/// The installed candidate CLI of the qualified prefix.
#[derive(Debug, Clone)]
pub struct Cli {
    prefix: PathBuf,
    executable: PathBuf,
    digest: String,
}

impl Cli {
    /// Resolve the qualified prefix once per process. Panics name the exact missing variable or file.
    pub fn qualified() -> &'static Cli {
        QUALIFIED.get_or_init(|| {
            for name in FORBIDDEN_OVERRIDES {
                if std::env::var_os(name).is_some_and(|value| !value.is_empty()) {
                    panic!("{name} is set; the release harness runs only the installed prefix and refuses developer overrides");
                }
            }
            let prefix = std::env::var_os(QUALIFIED_PREFIX_ENV)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| panic!("{QUALIFIED_PREFIX_ENV} must name the installed candidate prefix"));
            assert!(prefix.is_absolute(), "{QUALIFIED_PREFIX_ENV} must be absolute, got {}", prefix.display());
            let executable = prefix.join("bin").join(executable_name("beskid"));
            let metadata = std::fs::symlink_metadata(&executable)
                .unwrap_or_else(|error| panic!("installed CLI {} is missing: {error}", executable.display()));
            assert!(metadata.is_file(), "installed CLI {} must be a regular file", executable.display());
            for required in ["beskid_corelib/CoreLib.bws", "beskid_corelib/.beskid-bundle.sha256", "lib/beskid-runtime"] {
                assert!(prefix.join(required).exists(), "installed prefix {} lacks {required}", prefix.display());
            }
            let bytes = std::fs::read(&executable).unwrap_or_else(|error| panic!("read installed CLI: {error}"));
            let digest = format!("{:x}", Sha256::digest(&bytes));
            Cli { prefix, executable, digest }
        })
    }

    pub fn prefix(&self) -> &Path {
        &self.prefix
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// `<prefix>/beskid_corelib`, the only Corelib the installed CLI may discover.
    pub fn corelib(&self) -> PathBuf {
        self.prefix.join("beskid_corelib")
    }

    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// A command for the installed CLI with a private home, no developer overrides and no network
    /// credentials. `home` isolates every per-user directory the CLI may touch.
    pub fn command(&self, home: &Home) -> Command {
        let mut command = Command::new(&self.executable);
        for name in FORBIDDEN_OVERRIDES {
            command.env_remove(name);
        }
        command
            .env("HOME", home.user_home())
            .env("USERPROFILE", home.user_home())
            .env("BESKID_HOME", home.toolchain_home())
            .env("BESKID_CONFIG_DIR", home.config())
            .env("OTEL_SDK_DISABLED", "true")
            .env_remove("BESKID_PCKG_TOKEN")
            .env_remove("BESKID_PCKG_API_KEY")
            // The registry comes only from the project or workspace under test.
            .env_remove("BESKID_PCKG_URL")
            .current_dir(home.work());
        command
    }

    /// A command whose Corelib is a tampered copy of the installed one. The compiler and runtime kit
    /// stay installed; this is the single sanctioned override, used only to prove that the installed
    /// CLI fails closed on a damaged required Mod (MOD-06-01).
    pub fn command_with_corelib_copy(&self, home: &Home, corelib_copy: &Path) -> Command {
        let mut command = self.command(home);
        command.env("BESKID_CORELIB_ROOT", corelib_copy);
        command
    }

    /// The installed CLI bytes copied into `prefix/bin` of an otherwise incomplete prefix, for the
    /// read-only `doctor` observation of an incomplete installation (CLI06-04). The digest, and so
    /// the invocation proof, stays that of the installed candidate.
    pub fn copied_into_incomplete_prefix(&self, prefix: &Path) -> Cli {
        let bin = prefix.join("bin");
        std::fs::create_dir_all(&bin).expect("incomplete prefix bin");
        let executable = bin.join(self.executable.file_name().expect("installed CLI name"));
        std::fs::copy(&self.executable, &executable).expect("copy installed CLI bytes");
        Cli { prefix: prefix.to_path_buf(), executable, digest: self.digest.clone() }
    }

    /// Spawn one installed invocation with piped streams, emitting the invocation proof line first.
    /// Callers that need to interleave with the child (external edits during a transaction) use this.
    pub fn spawn(&self, command: &mut Command) -> std::process::Child {
        self.prove_invocation();
        command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        command.spawn().unwrap_or_else(|error| panic!("spawn installed CLI {}: {error}", self.executable.display()))
    }

    /// Run one installed invocation to completion, emitting the invocation proof line first.
    pub fn run(&self, command: &mut Command) -> Output {
        self.run_with_stdin(command, None)
    }

    /// [`Cli::run`] with bytes written to the child's stdin (or a closed stdin when `None`).
    pub fn run_with_stdin(&self, command: &mut Command, stdin: Option<&[u8]>) -> Output {
        self.prove_invocation();
        command.stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() });
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = command.spawn().unwrap_or_else(|error| panic!("spawn installed CLI {}: {error}", self.executable.display()));
        if let Some(bytes) = stdin {
            let mut pipe = child.stdin.take().expect("piped stdin");
            // The child may exit before reading; a closed pipe is not a harness failure.
            let _ = pipe.write_all(bytes);
        }
        let started = Instant::now();
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let out = std::thread::spawn(move || read_all(stdout));
        let err = std::thread::spawn(move || read_all(stderr));
        let status = loop {
            if let Some(status) = child.try_wait().expect("wait for installed CLI") {
                break status;
            }
            if started.elapsed() > INVOCATION_LIMIT {
                let _ = child.kill();
                let _ = child.wait();
                panic!("installed CLI exceeded {}s", INVOCATION_LIMIT.as_secs());
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        Output { status, stdout: out.join().expect("stdout reader"), stderr: err.join().expect("stderr reader") }
    }

    /// Write the proof line directly to the process stderr. libtest only captures the `print!`
    /// family, so the line reaches the producer whether or not `--nocapture` is given.
    fn prove_invocation(&self) {
        let name = self.executable.file_name().and_then(OsStr::to_str).expect("installed CLI name");
        let line = format!("beskid-installed-invocation sha256={} argv0={name}\n", self.digest);
        let stderr = std::io::stderr();
        let mut lock = stderr.lock();
        lock.write_all(line.as_bytes()).expect("write invocation proof");
        lock.flush().expect("flush invocation proof");
    }
}

fn read_all(mut reader: impl std::io::Read) -> Vec<u8> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).expect("read child stream");
    bytes
}

pub fn executable_name(stem: &str) -> String {
    if cfg!(windows) { format!("{stem}.exe") } else { stem.to_owned() }
}

/// A private, fresh per-test home: user home, toolchain home, configuration and a working directory.
#[derive(Debug)]
pub struct Home {
    root: tempfile::TempDir,
}

impl Home {
    pub fn new() -> Self {
        let root = tempfile::Builder::new().prefix("beskid-release-").tempdir().expect("private home");
        for directory in ["user-home", "toolchain-home", "config", "work"] {
            std::fs::create_dir_all(root.path().join(directory)).expect("private home directory");
        }
        Self { root }
    }

    pub fn root(&self) -> &Path {
        self.root.path()
    }
    pub fn user_home(&self) -> PathBuf {
        self.root.path().join("user-home")
    }
    pub fn toolchain_home(&self) -> PathBuf {
        self.root.path().join("toolchain-home")
    }
    pub fn config(&self) -> PathBuf {
        self.root.path().join("config")
    }
    /// The working directory of installed invocations; projects live below it.
    pub fn work(&self) -> PathBuf {
        self.root.path().join("work")
    }

    /// True when the CLI provisioned anything into its per-user directories.
    pub fn has_provisioning_writes(&self) -> bool {
        for directory in [self.user_home(), self.toolchain_home(), self.config()] {
            if std::fs::read_dir(&directory).map(|mut entries| entries.next().is_some()).unwrap_or(false) {
                return true;
            }
        }
        false
    }
}

impl Default for Home {
    fn default() -> Self {
        Self::new()
    }
}

/// Lossy text of both streams for diagnostics.
pub fn transcript(output: &Output) -> String {
    format!("exit: {:?}\nstdout:\n{}\nstderr:\n{}", output.status.code(), String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
}

/// True when the bytes contain an ANSI escape or an in-place carriage return (a `\r` that is not
/// part of a CRLF line ending).
pub fn has_cursor_controls(bytes: &[u8]) -> bool {
    bytes.contains(&0x1b) || bytes.iter().enumerate().any(|(index, byte)| *byte == b'\r' && bytes.get(index + 1) != Some(&b'\n'))
}
