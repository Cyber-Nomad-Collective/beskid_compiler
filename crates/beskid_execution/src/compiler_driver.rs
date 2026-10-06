//! An owned Cargo Rustc/linker wrapper. Every receipt comes from an actual
//! bounded shared-execution invocation, never configured-path metadata.
use crate::NativeExecutionControl;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompilerDriverTool {
    pub executable: PathBuf,
    pub sha256: String,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompilerDriverConfiguration {
    pub version: u32,
    pub nonce: String,
    pub rustc: CompilerDriverTool,
    pub linker: CompilerDriverTool,
    pub receipts: PathBuf,
    pub remaining_millis: u64,
    pub output_limit: u64,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompilerDriverReceipt {
    pub version: u32,
    pub nonce: String,
    pub phase: String,
    pub executable: PathBuf,
    pub sha256: String,
    pub arguments: Vec<String>,
    pub success: bool,
    pub exit_code: Option<i32>,
}
fn error(message: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_string())
}
fn hash_file(path: &Path) -> io::Result<String> {
    let file = fs::File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > 512 * 1024 * 1024 {
        return Err(error("compiler driver tool is not a bounded regular file"));
    }
    let mut reader = std::io::BufReader::new(file);
    let mut hash = Sha256::new();
    let mut count = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        let n = std::io::Read::read(&mut reader, &mut buffer)?;
        if n == 0 {
            break;
        }
        count += n as u64;
        if count > 512 * 1024 * 1024 {
            return Err(error("compiler driver tool grew beyond bound"));
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn verify(tool: &CompilerDriverTool) -> io::Result<PathBuf> {
    let path = fs::canonicalize(&tool.executable)?;
    if path != tool.executable || tool.sha256.len() != 64 || hash_file(&path)? != tool.sha256 {
        return Err(error("compiler driver configured tool differs"));
    }
    Ok(path)
}
fn inherited_enclosure() -> io::Result<()> {
    if std::env::var("BESKID_EXECUTION_ENCLOSURE").as_deref() != Ok("owned") {
        return Err(error("compiler driver requires the producer's owned execution enclosure"));
    }
    #[cfg(unix)]
    unsafe {
        let group = libc::getpgrp();
        if group <= 0 || group == libc::getpid() || libc::getpgid(libc::getppid()) != group {
            return Err(error("compiler driver is not inside its retained parent process group"));
        }
    }
    #[cfg(windows)]
    unsafe {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetCurrentProcess() -> *mut std::ffi::c_void;
            fn IsProcessInJob(process: *mut std::ffi::c_void, job: *mut std::ffi::c_void, result: *mut i32) -> i32;
        }
        let mut result = 0;
        if IsProcessInJob(GetCurrentProcess(), std::ptr::null_mut(), &mut result) == 0 || result == 0 {
            return Err(error("compiler driver is not inside its retained parent job"));
        }
    }
    #[cfg(not(any(unix, windows)))]
    return Err(error("compiler driver containment unsupported"));
    Ok(())
}
/// Entry point only for the installed, compiler-owned driver executable. Its
/// private configuration is produced by the source-compilation issuer.
pub fn run_compiler_driver(arguments: Vec<OsString>) -> io::Result<i32> {
    inherited_enclosure()?;
    let config_path = PathBuf::from(
        std::env::var_os("BESKID_NATIVE_COMPILER_DRIVER_CONFIG")
            .ok_or_else(|| error("compiler driver configuration absent"))?,
    );
    let metadata = fs::symlink_metadata(&config_path)?;
    if !metadata.is_file() || metadata.len() > 65536 {
        return Err(error("compiler driver configuration invalid"));
    }
    let config_path = fs::canonicalize(config_path)?;
    let bytes = fs::read(&config_path)?;
    let config: CompilerDriverConfiguration = serde_json::from_slice(&bytes).map_err(error)?;
    if config.version != 1
        || config.nonce.len() != 64
        || !config.nonce.bytes().all(|c| c.is_ascii_hexdigit())
        || config.remaining_millis == 0
        || config.remaining_millis > 600000
        || config.output_limit == 0
        || config.output_limit > 8 * 1024 * 1024
    {
        return Err(error("compiler driver configuration bounds/version invalid"));
    }
    let directory = fs::canonicalize(&config.receipts)?;
    if directory != config.receipts || directory.parent() != config_path.parent() {
        return Err(error("compiler driver receipt directory outside private configuration"));
    }
    if arguments.len() > 8192 {
        return Err(error("compiler driver argument count bound"));
    }
    let rustc = verify(&config.rustc)?;
    let linker = verify(&config.linker)?;
    let (phase, tool, args) = if arguments.first().is_some_and(|arg| Path::new(arg) == rustc) {
        ("rustc", &config.rustc, &arguments[1..])
    } else {
        ("linker", &config.linker, arguments.as_slice())
    };
    let argv = args
        .iter()
        .map(|arg| arg.to_str().map(str::to_owned).ok_or_else(|| error("compiler driver non-UTF8 argument")))
        .collect::<io::Result<Vec<_>>>()?;
    if argv.iter().map(String::len).sum::<usize>() > 1024 * 1024 {
        return Err(error("compiler driver argument bytes bound"));
    }
    let mut command = std::process::Command::new(&tool.executable);
    command.args(args).env_remove("RUSTC_WRAPPER").env_remove("RUSTC_WORKSPACE_WRAPPER");
    let cancellation = Arc::new(|| false);
    // The enclosing Cargo control retains the same absolute deadline, including
    // startup and nested calls. This nested timer can only shorten its budget.
    let control = unsafe {
        NativeExecutionControl::new(Instant::now() + Duration::from_millis(config.remaining_millis), cancellation)
            .with_inherited_enclosure()
    }
    .with_output_limit(config.output_limit)
    .map_err(error)?;
    let output = control.run_command(&mut command, &directory, phase).map_err(error)?;
    verify(tool)?;
    if fs::read(&config_path)? != bytes {
        return Err(error("compiler driver configuration mutated during invocation"));
    }
    let receipt = CompilerDriverReceipt {
        version: 1,
        nonce: config.nonce,
        phase: phase.into(),
        executable: tool.executable.clone(),
        sha256: tool.sha256.clone(),
        arguments: argv,
        success: output.status.success(),
        exit_code: output.status.code(),
    };
    let receipt_bytes = serde_json::to_vec(&receipt).map_err(error)?;
    if receipt_bytes.len() > 2 * 1024 * 1024 {
        return Err(error("compiler driver receipt bound"));
    }
    let mut file = tempfile::NamedTempFile::new_in(&directory)?;
    file.write_all(&receipt_bytes)?;
    file.as_file().sync_all()?;
    file.persist_noclobber(directory.join(format!(
        "{}-{}-{}.json",
        phase,
        std::process::id(),
        format!("{:x}", Sha256::digest(&receipt_bytes))
    )))
    .map_err(|error| error.error)?;
    io::stdout().write_all(&output.stdout)?;
    io::stderr().write_all(&output.stderr)?;
    Ok(output.status.code().unwrap_or(101))
}
