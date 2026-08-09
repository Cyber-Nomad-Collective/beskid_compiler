use std::process::Command;

use crate::error::{AotError, AotResult};

use super::common::format_link_detail;
use super::{LinkRequest, LinkResult};

pub(super) fn archive_static_libtool(req: &LinkRequest) -> AotResult<LinkResult> {
    if req.runtime_staticlib.is_none() {
        let output = Command::new("libtool")
            .arg("-static")
            .arg("-o")
            .arg(&req.output_path)
            .arg(&req.object_path)
            .args(&req.additional_object_paths)
            .output()
            .map_err(|_| AotError::LinkerUnavailable)?;
        if !output.status.success() {
            return Err(AotError::LinkFailed {
                status: output.status.code().unwrap_or(-1),
                command: format!("libtool -static -o {} {}", req.output_path.display(), req.object_path.display()),
                detail: format_link_detail(&output),
            });
        }
        return Ok(LinkResult {
            output_path: req.output_path.clone(),
            command_line: format!("libtool -static -o {} {}", req.output_path.display(), req.object_path.display()),
            exported_symbols: req.exported_symbols.clone(),
        });
    }
    let runtime_lib = req.runtime_staticlib.as_ref().ok_or_else(|| AotError::InvalidRequest {
        message: "static archive output requires runtime archive unless standalone object-only mode is used".to_owned(),
    })?;

    let mut cmd = Command::new("libtool");
    cmd.arg("-static");
    cmd.arg("-o").arg(&req.output_path);
    cmd.arg(runtime_lib);
    cmd.arg(&req.object_path);
    cmd.args(&req.additional_object_paths);

    if req.verbose {
        eprintln!("[aot] archive command: {:?}", cmd);
    }

    let output = cmd.output().map_err(|_| AotError::LinkerUnavailable)?;
    if !output.status.success() {
        return Err(AotError::LinkFailed {
            status: output.status.code().unwrap_or(-1),
            command: format!(
                "libtool -static -o {} {} {}",
                req.output_path.display(),
                runtime_lib.display(),
                req.object_path.display()
            ),
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
        command_line: format!(
            "libtool -static -o {} {} {} && ranlib {}",
            req.output_path.display(),
            runtime_lib.display(),
            req.object_path.display(),
            req.output_path.display()
        ),
        exported_symbols: req.exported_symbols.clone(),
    })
}
