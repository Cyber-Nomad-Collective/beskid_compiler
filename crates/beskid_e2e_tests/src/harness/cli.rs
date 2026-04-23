use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static CORELIB_ROOT_COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct BeskidCliInvoker {
    binary: PathBuf,
    corelib_root: PathBuf,
}

impl BeskidCliInvoker {
    pub fn new() -> Self {
        let corelib_root = unique_corelib_root();
        fs::create_dir_all(&corelib_root).unwrap_or_else(|error| {
            panic!(
                "create e2e corelib root {}: {error}",
                corelib_root.display()
            )
        });
        Self {
            binary: resolve_cli_binary(),
            corelib_root,
        }
    }

    pub fn command<I, S>(&self, args: I) -> Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut command = Command::new(&self.binary);
        command.env("BESKID_CORELIB_ROOT", &self.corelib_root);
        for argument in args {
            command.arg(argument.as_ref());
        }
        command
    }

    pub fn run<I, S>(&self, args: I) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.command(args).output().expect("run Beskid CLI command")
    }
}

fn resolve_cli_binary() -> PathBuf {
    if let Ok(path) = std::env::var("BESKID_CLI_BIN") {
        let binary = PathBuf::from(path);
        assert!(
            binary.is_file(),
            "BESKID_CLI_BIN points to non-existent file: {}",
            binary.display()
        );
        return binary;
    }

    let fallback = default_binary_path();
    assert!(
        fallback.is_file(),
        "Beskid CLI binary not found at {}. Build it first (`cargo build -p beskid_cli`) or set BESKID_CLI_BIN.",
        fallback.display()
    );
    fallback
}

fn default_binary_path() -> PathBuf {
    workspace_root()
        .join("target")
        .join("debug")
        .join(binary_name())
}

fn workspace_root() -> PathBuf {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    crate_root
        .parent()
        .expect("crate parent")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "beskid_cli.exe"
    } else {
        "beskid_cli"
    }
}

fn unique_corelib_root() -> PathBuf {
    let nonce = CORELIB_ROOT_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir()
        .join("beskid_e2e_corelib")
        .join(format!("{}_{}", std::process::id(), nonce))
}
