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
    if let Some(runtime_staticlib) = &req.runtime_staticlib
        && !runtime_staticlib.exists()
    {
        return Err(AotError::RuntimeArchiveMissing { path: runtime_staticlib.clone() });
    }

    if let Some(parent) = req.output_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| AotError::Io { path: parent.to_path_buf(), message: err.to_string() })?;
    }

    if req.output_kind == BuildOutputKind::StaticLib {
        return archive_static(req);
    }

    let compiler = detect_c_compiler();
    let target = req.target_triple.as_deref().unwrap_or(std::env::consts::OS).to_ascii_lowercase();
    if target.contains("windows") {
        return link_windows(req, &target);
    }
    let mut cmd = Command::new(&compiler);
    cmd.arg(&req.object_path);
    cmd.args(&req.additional_object_paths);
    if let Some(runtime_staticlib) = &req.runtime_staticlib {
        append_static_archive(&mut cmd, &target, runtime_staticlib);
    }
    if let Some(host_staticlib) = &req.host_staticlib {
        append_static_archive(&mut cmd, &target, host_staticlib);
    }
    cmd.arg("-o").arg(&req.output_path);
    append_library_search_paths(req, &target, &mut cmd)?;
    append_external_libraries(req, &target, &mut cmd)?;

    if matches!(req.output_kind, BuildOutputKind::SharedLib) {
        cmd.arg("-shared");
        if let LinkMode::PreferStatic = req.link_mode {
            cmd.arg("-Wl,-Bstatic");
        }
        if let LinkMode::PreferDynamic = req.link_mode {
            cmd.arg("-Wl,-Bdynamic");
        }
        append_export_policy_flags(req, &target, &mut cmd)?;
    }

    if req.verbose {
        eprintln!("[aot] link command: {:?}", cmd);
    }

    let output = cmd.output().map_err(|_| AotError::LinkerUnavailable)?;

    if !output.status.success() {
        return Err(AotError::LinkFailed {
            status: output.status.code().unwrap_or(-1),
            command: format_link_command(&compiler, req, &target),
            detail: format_link_detail(&output),
        });
    }

    Ok(LinkResult {
        output_path: req.output_path.clone(),
        command_line: format_link_command(&compiler, req, &target),
        exported_symbols: req.exported_symbols.clone(),
    })
}
