//! Build a linked executable from a codegen artifact and run it in a subprocess.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use beskid_codegen::CodegenArtifact;

use crate::api::{
    AotBuildRequest, BuildOutputKind, BuildProfile, ExportPolicy, LinkMode, RuntimeKitRequest,
    build,
};
use crate::error::{AotError, AotResult};
use crate::target::{detect_target, output_filename};

const RUN_EXE_BASENAME: &str = "beskid_run";
const DEFAULT_RUN_TIMEOUT: Duration = Duration::from_secs(60);

/// Inputs for [`build_and_run`]: lowered artifact, entrypoint, output directory, and runtime strategy.
#[derive(Debug, Clone)]
pub struct AotRunRequest {
    pub artifact: CodegenArtifact,
    pub entrypoint: String,
    pub output_dir: PathBuf,
    pub runtime: RuntimeKitRequest,
}

/// Linked executable path plus captured subprocess outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AotRunResult {
    pub exe_path: PathBuf,
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Emit and link an executable under `request.output_dir`, then run it with a bounded wait.
pub fn build_and_run(request: AotRunRequest) -> AotResult<AotRunResult> {
    if request.entrypoint.trim().is_empty() {
        return Err(AotError::InvalidRequest {
            message: "entrypoint must not be empty".to_owned(),
        });
    }

    std::fs::create_dir_all(&request.output_dir).map_err(|err| AotError::Io {
        path: request.output_dir.clone(),
        message: err.to_string(),
    })?;

    let target = detect_target(None)?;
    let exe_path = request.output_dir.join(output_filename(
        RUN_EXE_BASENAME,
        BuildOutputKind::Exe,
        &target,
    ));

    let build_result = build(AotBuildRequest {
        artifact: request.artifact,
        output_kind: BuildOutputKind::Exe,
        output_path: exe_path.clone(),
        object_path: None,
        target_triple: None,
        profile: BuildProfile::Debug,
        entrypoint: request.entrypoint,
        export_policy: ExportPolicy::PublicOnly,
        link_mode: LinkMode::Auto,
        runtime: Some(request.runtime),
        verbose_link: false,
        external_libraries: Vec::new(),
        library_search_paths: Vec::new(),
        pipeline: None,
    })?;

    let exe_path = build_result
        .final_path
        .ok_or_else(|| AotError::InvalidRequest {
            message: "executable build did not produce a final linked artifact".to_owned(),
        })?;

    let (exit_code, stdout, stderr) = run_executable(&exe_path, DEFAULT_RUN_TIMEOUT)?;

    Ok(AotRunResult {
        exe_path,
        exit_code,
        stdout,
        stderr,
    })
}

/// Run an already-linked executable produced by [`build`] or [`build_and_run`].
pub fn run_linked_executable(path: &Path) -> AotResult<AotRunResult> {
    let (exit_code, stdout, stderr) = run_executable(path, DEFAULT_RUN_TIMEOUT)?;
    Ok(AotRunResult {
        exe_path: path.to_path_buf(),
        exit_code,
        stdout,
        stderr,
    })
}

fn run_executable(path: &Path, timeout: Duration) -> AotResult<(i32, Vec<u8>, Vec<u8>)> {
    let mut child = Command::new(path)
        .env("BESKID_AOT_MAIN", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| AotError::RunFailed {
            path: path.to_path_buf(),
            message: err.to_string(),
        })?;

    let start = Instant::now();
    loop {
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AotError::RunTimedOut {
                path: path.to_path_buf(),
                seconds: timeout.as_secs(),
            });
        }

        match child.try_wait().map_err(|err| AotError::RunFailed {
            path: path.to_path_buf(),
            message: err.to_string(),
        })? {
            Some(_) => {
                let output = child
                    .wait_with_output()
                    .map_err(|err| AotError::RunFailed {
                        path: path.to_path_buf(),
                        message: err.to_string(),
                    })?;
                let exit_code = output.status.code().unwrap_or(-1);
                return Ok((exit_code, output.stdout, output.stderr));
            }
            None => thread::sleep(Duration::from_millis(20)),
        }
    }
}
