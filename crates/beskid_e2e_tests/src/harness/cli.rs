use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use beskid_abi::abi_v5::TargetMetadata;
use beskid_abi::runtime_kit::BuildProfile;
use beskid_abi::runtime_source::{canonical_runtime_source_hash, resolve_canonical_runtime_kit};

static CORELIB_ROOT_COUNTER: AtomicU64 = AtomicU64::new(0);
static BUILT_DEBUG_CLI: OnceLock<()> = OnceLock::new();
static STAGED_RUNTIME_KIT: OnceLock<Mutex<HashMap<String, Result<PathBuf, String>>>> = OnceLock::new();

pub struct BeskidCliInvoker {
    binary: PathBuf,
    corelib_root: PathBuf,
    runtime_prefix: PathBuf,
}

impl BeskidCliInvoker {
    pub fn new() -> Self {
        let binary = resolve_cli_binary();
        let runtime_prefix = ensure_exact_runtime_kit(&binary, "debug");
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
        let mut release = false;
        for argument in args {
            let argument = argument.as_ref();
            release |= argument == "--release";
            command.arg(argument);
        }
        if release {
            ensure_exact_runtime_kit(&self.binary, "release");
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

/// Stage the exact host kit for the requested build profile when missing.
///
/// Missing kits remain fail-closed for consumers that do not go through this harness.
/// This only publishes through `dev runtime-kit build-native-host` — no prebuilt/search fallback.
/// The first outcome per profile (success or failure) is recorded and replayed to every caller,
/// so one staging failure is reported with its original error instead of poisoning later tests.
fn ensure_exact_runtime_kit(cli_binary: &Path, profile: &str) -> PathBuf {
    let staged = STAGED_RUNTIME_KIT.get_or_init(|| Mutex::new(HashMap::new()));
    // The guard is never held across a panic; recover regardless so a stray poison cannot cascade.
    let mut outcomes = staged.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let outcome = outcomes
        .entry(profile.to_string())
        .or_insert_with(|| stage_exact_runtime_kit(cli_binary, profile))
        .clone();
    drop(outcomes);
    outcome.unwrap_or_else(|error| panic!("{error}"))
}

fn stage_exact_runtime_kit(cli_binary: &Path, profile: &str) -> Result<PathBuf, String> {
    let triple = host_abi_v5_triple()
        .map_err(|host| format!("e2e CLI harness requires a supported ABI-v5 host; got {host}"))?;
    let target = TargetMetadata::for_triple(triple)
        .map_err(|error| format!("host ABI-v5 target `{triple}` is unsupported: {error:?}"))?;
    let build_profile =
        BuildProfile::parse(profile)
        .map_err(|error| format!("harness profile `{profile}` is not debug or release: {error:?}"))?;
    // A shared Cargo target directory may retain kits from an older compiler source state.
    // Never use its unkeyed runtime-kit root as evidence for this compiler's embedded corpus.
    let prefix = install_prefix_for_cli(cli_binary);
    let metadata = prefix.join("lib/beskid-runtime/abi-5").join(triple).join(profile).join("abi.json");
    if metadata.is_file() {
        resolve_canonical_runtime_kit(&prefix, &target, build_profile).map_err(|error| {
            format!("existing e2e runtime kit `{}` is not exact: {error:?}", prefix.display())
        })?;
        return Ok(prefix);
    }

    let prefix_text = prefix.to_str().ok_or_else(|| format!("install prefix `{}` is not UTF-8", prefix.display()))?;
    let output = Command::new(cli_binary)
        .args(["dev", "runtime-kit", "build-native-host", "--prefix", prefix_text, "--profile", profile])
        .output()
        .map_err(|error| {
            format!(
                "invoke `{} dev runtime-kit build-native-host` to stage the exact {profile} kit: {error}",
                cli_binary.display()
            )
        })?;
    if !output.status.success() {
        return Err(format!(
            "staging exact {profile} ABI-v5 runtime kit failed for prefix `{}`\nstdout:\n{}\nstderr:\n{}",
            prefix.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    resolve_canonical_runtime_kit(&prefix, &target, build_profile)
        .map_err(|error| format!("staged e2e runtime kit `{}` is not exact: {error:?}", prefix.display()))?;
    Ok(prefix)
}

fn install_prefix_for_cli(cli_binary: &Path) -> PathBuf {
    let bin =
        cli_binary.parent().unwrap_or_else(|| panic!("CLI binary has no parent directory: {}", cli_binary.display()));
    let target = bin.parent().unwrap_or_else(|| panic!("CLI binary has no install prefix: {}", cli_binary.display()));
    target.join("beskid-e2e-runtime-kits").join(canonical_runtime_source_hash())
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
    cargo_target_dir().join("debug").join(binary_name())
}

/// Target directory the nested `cargo build -p beskid_cli` writes to.
///
/// The nested build inherits this process environment, so an explicit `CARGO_TARGET_DIR`
/// (absolute, or relative to the workspace where the nested build runs) selects the same
/// directory Cargo uses. Without it Cargo writes to `<workspace>/target`.
fn cargo_target_dir() -> PathBuf {
    target_dir_from(std::env::var_os("CARGO_TARGET_DIR"), &workspace_root())
}

fn target_dir_from(configured: Option<std::ffi::OsString>, workspace: &Path) -> PathBuf {
    match configured.filter(|value| !value.is_empty()).map(PathBuf::from) {
        Some(dir) if dir.is_absolute() => dir,
        Some(dir) => workspace.join(dir),
        None => workspace.join("target"),
    }
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
    fn self_staged_runtime_prefix_is_keyed_to_embedded_sources() {
        let root = tempfile::tempdir().expect("temporary target directory");
        let cli = root.path().join("debug").join(binary_name());
        assert_eq!(
            install_prefix_for_cli(&cli),
            root.path().join("beskid-e2e-runtime-kits").join(canonical_runtime_source_hash()),
            "a shared target's unkeyed runtime kit may contain stale abi.json"
        );
    }

    #[test]
    fn default_cli_build_command_targets_the_workspace_cli_package() {
        let command = build_current_cli_command(&workspace_root());

        assert_eq!(command.get_program(), "cargo");
        assert_eq!(command.get_args().collect::<Vec<_>>(), ["build", "-p", "beskid_cli"].map(std::ffi::OsStr::new));
        assert_eq!(command.get_current_dir(), Some(workspace_root().as_path()));
    }

    #[test]
    fn default_cli_path_follows_the_cargo_target_dir_of_the_nested_build() {
        let workspace = std::env::temp_dir().join("compiler");
        let absolute = std::env::temp_dir().join("cache-env");

        assert_eq!(target_dir_from(None, &workspace), workspace.join("target"));
        assert_eq!(target_dir_from(Some("".into()), &workspace), workspace.join("target"));
        assert_eq!(target_dir_from(Some(absolute.clone().into_os_string()), &workspace), absolute);
        assert_eq!(target_dir_from(Some("out/env".into()), &workspace), workspace.join("out/env"));
    }
}
