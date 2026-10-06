use std::process::Command;

use crate::api::{BuildOutputKind, LinkMode};
use crate::error::{AotError, AotResult};

use super::common::{append_static_archive, detect_c_compiler, format_link_command, format_link_detail};
use super::policy::{append_export_policy_flags, append_external_libraries, append_library_search_paths};
use super::unix::archive_static;
use super::windows::link_windows;
use super::{LinkRequest, LinkResult};

/// Link or merge into `req.output_path` using the host toolchain (see module docs for platform notes).
pub fn link(req: &LinkRequest) -> AotResult<LinkResult> {
    link_with_control(req, None)
}

pub fn link_with_control(
    req: &LinkRequest,
    control: Option<&crate::api::NativeExecutionControl>,
) -> AotResult<LinkResult> {
    if let Some(control) = control {
        control.check("link")?;
    }
    if !req.object_path.exists() {
        return Err(AotError::Io { path: req.object_path.clone(), message: "object file does not exist".to_owned() });
    }
    for object_path in &req.additional_object_paths {
        if !object_path.exists() {
            return Err(AotError::Io {
                path: object_path.clone(),
                message: "additional object file does not exist".to_owned(),
            });
        }
    }
    if let Some(runtime) = &req.runtime {
        if !runtime.path().is_file() {
            return Err(AotError::RuntimeArchiveMissing { path: runtime.path().to_path_buf() });
        }
        if let super::RuntimeLinkInput::GlueSharedProviderV1 { shared_library_path, .. } = runtime {
            if !shared_library_path.is_file() {
                return Err(AotError::Io {
                    path: shared_library_path.clone(),
                    message: "validated shared runtime payload is missing".into(),
                });
            }
            if req.output_kind == BuildOutputKind::StaticLib {
                return Err(AotError::InvalidRequest {
                    message: "shared runtime provider cannot be merged into static output".into(),
                });
            }
        }
    }

    if let Some(parent) = req.output_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| AotError::Io { path: parent.to_path_buf(), message: err.to_string() })?;
    }

    if req.output_kind == BuildOutputKind::StaticLib {
        if control.is_some() {
            return Err(AotError::InvalidRequest {
                message: "bounded native test linking requires executable output".into(),
            });
        }
        return archive_static(req);
    }

    let compiler = detect_c_compiler();
    let target = req.target_triple.as_deref().unwrap_or(std::env::consts::OS).to_ascii_lowercase();
    if target.contains("windows") {
        return link_windows(req, &target, control);
    }
    let cmd = unix_link_command(req, &target, &compiler)?;

    if req.verbose {
        eprintln!("[aot] link command: {:?}", cmd);
    }

    let (output, tool_receipt) = super::run_link_tool(
        &super::LinkToolInvocation::from_link_command(&cmd),
        req.output_path.parent().unwrap_or(std::path::Path::new(".")),
        control,
    )?;

    if !output.status.success() {
        return Err(AotError::LinkFailed {
            status: output.status.code().unwrap_or(-1),
            command: format_link_command(&cmd),
            detail: format_link_detail(&output),
        });
    }

    Ok(LinkResult {
        tool_receipt: Some(tool_receipt),
        output_path: req.output_path.clone(),
        command_line: format_link_command(&cmd),
        exported_symbols: req.exported_symbols.clone(),
    })
}

pub(crate) fn unix_link_command(req: &LinkRequest, target: &str, compiler: &str) -> AotResult<Command> {
    let mut cmd = Command::new(compiler);
    cmd.arg(&req.object_path);
    cmd.args(&req.additional_object_paths);
    if let Some(runtime) = &req.runtime {
        match runtime {
            super::RuntimeLinkInput::CanonicalStatic { archive } => append_static_archive(&mut cmd, target, archive),
            super::RuntimeLinkInput::GlueSharedProviderV1 { link_path, shared_library_path } => {
                cmd.arg(link_path);
                let loader_directory = shared_library_path.parent().ok_or_else(|| AotError::InvalidRequest {
                    message: "shared provider has no loader directory".into(),
                })?;
                let directory = loader_directory.to_str().ok_or_else(|| AotError::InvalidRequest {
                    message: "shared provider loader directory is not UTF-8".into(),
                })?;
                if directory.is_empty() || directory.contains(',') {
                    return Err(AotError::InvalidRequest {
                        message: "shared provider loader directory cannot be passed safely to the linker".into(),
                    });
                }
                cmd.arg(format!("-Wl,-rpath,{directory}"));
            }
        }
    }
    if let Some(host_staticlib) = &req.host_staticlib {
        append_static_archive(&mut cmd, target, host_staticlib);
    }
    cmd.arg("-o").arg(&req.output_path);
    append_library_search_paths(req, target, &mut cmd)?;
    append_external_libraries(req, target, &mut cmd)?;

    if matches!(req.output_kind, BuildOutputKind::SharedLib) {
        cmd.arg("-shared");
        let filename = req.output_path.file_name().and_then(|name| name.to_str()).ok_or_else(|| {
            AotError::InvalidRequest { message: "shared library requires a UTF-8 loader basename".into() }
        })?;
        if filename.is_empty()
            || !filename.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
        {
            return Err(AotError::InvalidRequest {
                message: "shared library loader basename contains unsupported characters".into(),
            });
        }
        // Installed providers are copied out of their build directory. Link their identity,
        // never an absolute staging pathname, into dependent binaries.
        if target.contains("darwin") || target.contains("macos") {
            cmd.arg(format!("-Wl,-install_name,@rpath/{filename}"));
        } else if target.contains("linux") {
            cmd.arg(format!("-Wl,-soname,{filename}"));
        }

        if let LinkMode::PreferStatic = req.link_mode {
            cmd.arg("-Wl,-Bstatic");
        }
        if let LinkMode::PreferDynamic = req.link_mode {
            cmd.arg("-Wl,-Bdynamic");
        }
        append_export_policy_flags(req, target, &mut cmd)?;
    }
    Ok(cmd)
}

#[cfg(test)]
mod runtime_input_v06_tests {
    use super::*;
    use crate::linker::RuntimeLinkInput;
    use std::path::PathBuf;

    #[test]
    fn v06_shared_runtime_link_input_uses_exact_provider_and_loader_directory() {
        let req = LinkRequest {
            target_triple: Some("x86_64-apple-darwin".into()),
            output_kind: BuildOutputKind::SharedLib,
            output_path: PathBuf::from("out/mod.dylib"),
            object_path: PathBuf::from("mod.o"),
            additional_object_paths: Vec::new(),
            runtime: Some(RuntimeLinkInput::GlueSharedProviderV1 {
                link_path: PathBuf::from("/validated/prefix/shared/libbeskid_runtime.dylib"),
                shared_library_path: PathBuf::from("/validated/prefix/shared/libbeskid_runtime.dylib"),
            }),
            host_staticlib: None,
            entrypoint_symbol: String::new(),
            exported_symbols: Vec::new(),
            link_mode: LinkMode::PreferDynamic,
            verbose: false,
            external_libraries: Vec::new(),
            library_search_paths: Vec::new(),
        };
        let command = unix_link_command(&req, "x86_64-apple-darwin", "cc").unwrap();
        let args: Vec<_> = command.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect();
        assert!(args.contains(&"/validated/prefix/shared/libbeskid_runtime.dylib".into()));
        assert!(args.contains(&"-Wl,-rpath,/validated/prefix/shared".into()));
        assert!(!args.iter().any(|arg| arg.contains("force_load") || arg.contains("whole-archive")));
    }
}
