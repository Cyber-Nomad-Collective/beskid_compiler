//! Bind native linker execution to a canonical executable and verify its bytes around invocation.
use crate::{
    api::NativeExecutionControl,
    error::{AotError, AotResult},
};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

#[derive(Debug, Clone)]
pub struct LinkToolReceipt {
    pub executable: PathBuf,
    pub sha256: String,
}

/// Native linker invocation with inherited environment and explicit overrides.
/// Standard streams belong to the native execution service, not the caller.
#[derive(Debug, Clone)]
pub struct LinkToolInvocation {
    pub program: std::ffi::OsString,
    pub args: Vec<std::ffi::OsString>,
    pub environment: Vec<(std::ffi::OsString, Option<std::ffi::OsString>)>,
    pub current_dir: Option<PathBuf>,
}

impl LinkToolInvocation {
    pub fn new(program: impl Into<std::ffi::OsString>) -> Self {
        Self { program: program.into(), args: Vec::new(), environment: Vec::new(), current_dir: None }
    }

    // Only platform-owned builders use this extraction. They always inherit environment
    // and configure arguments/overrides; no arbitrary public Command conversion exists.
    pub(super) fn from_link_command(command: &Command) -> Self {
        Self {
            program: command.get_program().to_os_string(),
            args: command.get_args().map(|arg| arg.to_os_string()).collect(),
            environment: command
                .get_envs()
                .map(|(key, value)| (key.to_os_string(), value.map(|value| value.to_os_string())))
                .collect(),
            current_dir: command.get_current_dir().map(PathBuf::from),
        }
    }

    fn command(&self, executable: &Path) -> Command {
        let mut command = Command::new(executable);
        command.args(&self.args);
        if let Some(directory) = &self.current_dir {
            command.current_dir(directory);
        }
        for (key, value) in &self.environment {
            match value {
                Some(value) => {
                    command.env(key, value);
                }
                None => {
                    command.env_remove(key);
                }
            }
        }
        command
    }
}

fn executable(invocation: &LinkToolInvocation) -> AotResult<PathBuf> {
    let program = Path::new(&invocation.program);
    let current = invocation
        .current_dir
        .clone()
        .unwrap_or(std::env::current_dir().map_err(|error| AotError::InvalidRequest { message: error.to_string() })?);
    let candidates = if program.is_absolute() {
        vec![program.to_path_buf()]
    } else if program.components().count() > 1 {
        vec![current.join(program)]
    } else {
        let override_path = invocation.environment.iter().rev().find(|(name, _)| {
            #[cfg(windows)]
            {
                name.eq_ignore_ascii_case("PATH")
            }
            #[cfg(not(windows))]
            {
                name == "PATH"
            }
        });
        let path = match override_path {
            Some((_, value)) => value.as_ref().map(|value| value.to_os_string()),
            None => std::env::var_os("PATH"),
        };
        let mut candidates = Vec::new();
        if let Some(path) = path {
            for directory in std::env::split_paths(&path) {
                let directory = if directory.is_absolute() { directory } else { current.join(directory) };
                let candidate = directory.join(program);
                candidates.push(candidate.clone());
                #[cfg(windows)]
                if candidate.extension().is_none() {
                    let extensions = invocation
                        .environment
                        .iter()
                        .rev()
                        .find(|(name, _)| name.eq_ignore_ascii_case("PATHEXT"))
                        .and_then(|(_, value)| value.as_ref().map(|value| value.to_os_string()))
                        .or_else(|| std::env::var_os("PATHEXT"))
                        .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".into());
                    for extension in extensions.to_string_lossy().split(';').filter(|extension| !extension.is_empty()) {
                        candidates.push(directory.join(format!("{}{}", program.to_string_lossy(), extension)));
                    }
                }
            }
        }
        candidates
    };
    for candidate in candidates {
        if !candidate.is_file() {
            continue;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if std::fs::metadata(&candidate).map(|metadata| metadata.permissions().mode() & 0o111 == 0).unwrap_or(true)
            {
                continue;
            }
        }
        return std::fs::canonicalize(&candidate)
            .map_err(|error| AotError::Io { path: candidate, message: error.to_string() });
    }
    Err(AotError::LinkerUnavailable)
}

fn digest(path: &Path) -> AotResult<String> {
    let bytes =
        std::fs::read(path).map_err(|error| AotError::Io { path: path.to_path_buf(), message: error.to_string() })?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub fn run_link_tool(
    invocation: &LinkToolInvocation,
    directory: &Path,
    control: Option<&NativeExecutionControl>,
) -> AotResult<(Output, LinkToolReceipt)> {
    if let Some(control) = control {
        control.check("link")?;
    }
    let executable = executable(invocation)?;
    let sha256 = digest(&executable)?;
    let mut resolved = invocation.command(&executable);
    let output = match control {
        Some(control) => control.run_command(&mut resolved, directory, "link")?,
        None => {
            resolved.output().map_err(|error| AotError::Io { path: executable.clone(), message: error.to_string() })?
        }
    };
    if digest(&executable)? != sha256 {
        return Err(AotError::InvalidRequest {
            message: format!("native link executable changed during invocation: {}", executable.display()),
        });
    }
    Ok((output, LinkToolReceipt { executable, sha256 }))
}
