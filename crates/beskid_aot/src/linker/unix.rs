use std::process::Command;

use crate::error::{AotError, AotResult};

use super::common::format_link_detail;
use super::macos::archive_static_libtool;
use super::windows::archive_static_windows;
use super::{LinkRequest, LinkResult};

pub(super) fn archive_static(req: &LinkRequest) -> AotResult<LinkResult> {
    let target = req.target_triple.as_deref().unwrap_or(std::env::consts::OS).to_ascii_lowercase();

    if target.contains("windows") {
        return archive_static_windows(req);
    }

    // macOS ships BSD `ar`, which does not implement GNU binutils MRI scripts (`ar -M`).
    // Xcode's `libtool -static` is the supported way to merge a static archive with objects.
    let is_apple_host_style = target.contains("darwin") || target.contains("apple") || target.contains("macos");
    if is_apple_host_style {
        return archive_static_libtool(req);
    }

    if req.runtime_staticlib.is_none() {
        let output = Command::new("ar")
            .arg("crs")
            .arg(&req.output_path)
            .arg(&req.object_path)
            .args(&req.additional_object_paths)
            .output()
            .map_err(|_| AotError::LinkerUnavailable)?;
        if !output.status.success() {
            return Err(AotError::LinkFailed {
                status: output.status.code().unwrap_or(-1),
                command: format!("ar crs {} {}", req.output_path.display(), req.object_path.display()),
                detail: format_link_detail(&output),
            });
        }
        return Ok(LinkResult {
            output_path: req.output_path.clone(),
            exported_symbols: req.exported_symbols.clone(),
            command_line: format!("ar crs {} {}", req.output_path.display(), req.object_path.display()),
        });
    }

    let script_path = req.output_path.with_extension("mri");
    let runtime_lib = req.runtime_staticlib.as_ref().expect("runtime checked above");
    let mut script = format!(
        "CREATE {}\nADDLIB {}\nADDMOD {}\n",
        req.output_path.display(),
        runtime_lib.display(),
        req.object_path.display()
    );
    for object in &req.additional_object_paths {
        script.push_str(&format!("ADDMOD {}\n", object.display()));
    }
    script.push_str("SAVE\nEND\n");
    std::fs::write(&script_path, script)
        .map_err(|err| AotError::Io { path: script_path.clone(), message: err.to_string() })?;

    let mut shell_command = Command::new("sh");
    shell_command.arg("-c").arg(format!("ar -M < {}", script_path.to_string_lossy()));

    if req.verbose {
        eprintln!("[aot] archive command: {:?}", shell_command);
    }

    let output = shell_command.output().map_err(|_| AotError::LinkerUnavailable)?;
    if !output.status.success() {
        return Err(AotError::LinkFailed {
            status: output.status.code().unwrap_or(-1),
            command: format!("ar -M < {}", script_path.display()),
            detail: format_link_detail(&output),
        });
    }

    let ranlib_out = Command::new("ranlib").arg(&req.output_path).output().map_err(|_| AotError::LinkerUnavailable)?;
    if !ranlib_out.status.success() {
        return Err(AotError::LinkFailed {
            status: ranlib_out.status.code().unwrap_or(-1),
            command: format!("ranlib {}", req.output_path.display()),
            detail: format_link_detail(&ranlib_out),
        });
    }

    Ok(LinkResult {
        output_path: req.output_path.clone(),
        command_line: format!("ar -M < {} && ranlib {}", script_path.display(), req.output_path.display()),
        exported_symbols: req.exported_symbols.clone(),
    })
}
