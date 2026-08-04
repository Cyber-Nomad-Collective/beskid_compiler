use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

static CORELIB_ROOT_COUNTER: AtomicU64 = AtomicU64::new(0);
static BUILT_DEBUG_CLI: OnceLock<()> = OnceLock::new();
static STAGED_DEBUG_KIT: OnceLock<Mutex<()>> = OnceLock::new();

pub struct BeskidCliInvoker {
    binary: PathBuf,
    corelib_root: PathBuf,
    runtime_prefix: PathBuf,
}

impl BeskidCliInvoker {
    pub fn new() -> Self {
        let binary = resolve_cli_binary();
        let runtime_prefix = ensure_exact_debug_runtime_kit(&binary);
        let corelib_root = unique_corelib_root();
        fs::create_dir_all(&corelib_root)
            .unwrap_or_else(|error| panic!("create e2e corelib root {}: {error}", corelib_root.display()));
        Self { binary, corelib_root, runtime_prefix }
    }

    pub fn command_in<I, S>(&self, working_dir: &Path, args: I) -> Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut command = Command::new(&self.binary);
        command
            .current_dir(working_dir)
            .env("BESKID_CORELIB_ROOT", &self.corelib_root)
            .env("BESKID_RUNTIME_PREFIX", &self.runtime_prefix);
        for argument in args {
            command.arg(argument.as_ref());
        }
        command
    }

    #[cfg(target_os = "linux")]
    pub fn command<I, S>(&self, args: I) -> Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.command_in(Path::new("."), args)
    }

    pub fn run_in<I, S>(&self, working_dir: &Path, args: I) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.command_in(working_dir, args).output().expect("run Beskid CLI command")
    }

    pub fn run<I, S>(&self, args: I) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.run_in(Path::new("."), args)
    }
}

/// Stage the exact host debug kit into the CLI install prefix when missing.
///
/// Missing kits remain fail-closed for consumers that do not go through this harness.
/// This only publishes through `runtime-kit build-native-host` — no prebuilt/search fallback.
fn ensure_exact_debug_runtime_kit(cli_binary: &Path) -> PathBuf {
    let prefix = install_prefix_for_cli(cli_binary);
    let lock = STAGED_DEBUG_KIT.get_or_init(|| Mutex::new(()));
    let _guard = lock.lock().expect("runtime-kit staging lock");

    let triple = host_abi_v5_triple()
        .unwrap_or_else(|host| panic!("e2e CLI harness requires a supported ABI-v5 host; got {host}"));
    let metadata = prefix.join("lib/beskid-runtime/abi-5").join(triple).join("debug").join("abi.json");
    if metadata.is_file() {
        return prefix;
    }

    let output = Command::new(cli_binary)
        .args([
            "runtime-kit",
            "build-native-host",
            "--prefix",
            prefix.to_str().expect("install prefix is UTF-8"),
            "--profile",
            "debug",
        ])
        .output()
        .unwrap_or_else(|error| {
            panic!(
                "invoke `{} runtime-kit build-native-host` to stage the exact debug kit: {error}",
                cli_binary.display()
            )
        });
    assert!(
        output.status.success(),
        "staging exact debug ABI-v5 runtime kit failed for prefix `{}`\nstdout:\n{}\nstderr:\n{}",
        prefix.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(metadata.is_file(), "exact debug kit metadata missing after staging: {}", metadata.display());
    prefix
}

fn install_prefix_for_cli(cli_binary: &Path) -> PathBuf {
    let bin =
        cli_binary.parent().unwrap_or_else(|| panic!("CLI binary has no parent directory: {}", cli_binary.display()));
    bin.parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| panic!("CLI binary has no install prefix: {}", cli_binary.display()))
}

fn host_abi_v5_triple() -> Result<&'static str, String> {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Ok("x86_64-unknown-linux-gnu"),
        ("aarch64", "macos") => Ok("aarch64-apple-darwin"),
        ("x86_64", "windows") => Ok("x86_64-pc-windows-msvc"),
        (arch, os) => Err(format!("{arch}-{os}")),
    }
}

fn resolve_cli_binary() -> PathBuf {
    if let Ok(path) = std::env::var("BESKID_CLI_BIN") {
        let binary = PathBuf::from(path);
        assert!(binary.is_file(), "BESKID_CLI_BIN points to non-existent file: {}", binary.display());
        return binary;
    }

    ensure_current_default_cli_binary();
    let fallback = default_binary_path();
    assert!(fallback.is_file(), "Beskid CLI binary missing after `cargo build -p beskid_cli`: {}", fallback.display());
    fallback
}

/// Build the default CLI through Cargo before the harness stages a runtime kit.
///
/// Cargo owns freshness through its dependency fingerprints, including generated ABI artifacts.
/// An explicit `BESKID_CLI_BIN` remains an override for callers that deliberately provide a
/// different executable.
fn ensure_current_default_cli_binary() {
    BUILT_DEBUG_CLI.get_or_init(|| {
        let workspace = workspace_root();
        let output = build_current_cli_command(&workspace)
            .output()
            .unwrap_or_else(|error| panic!("invoke `cargo build -p beskid_cli` for e2e harness: {error}"));
        assert!(
            output.status.success(),
            "building current Beskid CLI for e2e harness failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    });
}

fn build_current_cli_command(workspace: &Path) -> Command {
    let mut command = Command::new("cargo");
    command.current_dir(workspace).args(["build", "-p", "beskid_cli"]);
    command
}

fn default_binary_path() -> PathBuf {
    workspace_root().join("target").join("debug").join(binary_name())
}

fn workspace_root() -> PathBuf {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    crate_root.parent().expect("crate parent").parent().expect("workspace root").to_path_buf()
}

fn binary_name() -> &'static str {
    if cfg!(target_os = "windows") { "beskid_cli.exe" } else { "beskid_cli" }
}

fn unique_corelib_root() -> PathBuf {
    let nonce = CORELIB_ROOT_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join("beskid_e2e_corelib").join(format!("{}_{}", std::process::id(), nonce))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_cli_build_command_targets_the_workspace_cli_package() {
        let command = build_current_cli_command(&workspace_root());

        assert_eq!(command.get_program(), "cargo");
        assert_eq!(command.get_args().collect::<Vec<_>>(), ["build", "-p", "beskid_cli"].map(std::ffi::OsStr::new));
        assert_eq!(command.get_current_dir(), Some(workspace_root().as_path()));
    }
}
