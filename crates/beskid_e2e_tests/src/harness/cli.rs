use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub struct BeskidCliInvoker {
    binary: PathBuf,
}

impl BeskidCliInvoker {
    pub fn new() -> Self {
        Self {
            binary: resolve_cli_binary(),
        }
    }

    pub fn command<I, S>(&self, args: I) -> Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut command = Command::new(&self.binary);
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
