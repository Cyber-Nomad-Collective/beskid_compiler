use cargo_cross::config::HostPlatform;

use std::path::Path;
use std::process::{Command, Output};

use crate::api::BuildOutputKind;

use super::LinkRequest;

pub(super) fn detect_c_compiler() -> String {
    if let Ok(value) = std::env::var("CC") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return trimmed.to_owned();
        }
    }

    let host = HostPlatform::detect();
    if host.is_windows() { "cl".to_owned() } else { "cc".to_owned() }
}

pub(super) fn append_static_archive(cmd: &mut Command, target: &str, archive: &Path) {
    if target.contains("darwin") || target.contains("macos") {
        cmd.arg("-Wl,-force_load").arg(archive);
    } else {
        cmd.arg(archive);
    }
}

pub(super) fn format_link_command(compiler: &str, req: &LinkRequest, target: &str) -> String {
    let mut command_line = format!("{} {}", compiler, req.object_path.display());
    for object in &req.additional_object_paths {
        command_line.push(' ');
        command_line.push_str(&object.display().to_string());
    }
    if let Some(runtime_staticlib) = &req.runtime_staticlib {
        if target.contains("darwin") || target.contains("macos") {
            command_line.push_str(" -Wl,-force_load ");
        } else {
            command_line.push(' ');
        }
        command_line.push_str(&runtime_staticlib.display().to_string());
    }
    if let Some(host_staticlib) = &req.host_staticlib {
        if target.contains("darwin") || target.contains("macos") {
            command_line.push_str(" -Wl,-force_load ");
        } else {
            command_line.push(' ');
        }
        command_line.push_str(&host_staticlib.display().to_string());
    }
    command_line.push_str(" -o ");
    command_line.push_str(&req.output_path.display().to_string());
    if req.output_kind == BuildOutputKind::SharedLib {
        command_line.push_str(" -shared");
    }
    command_line
}

pub(super) fn format_link_detail(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stderr.trim().is_empty() && stdout.trim().is_empty() {
        return String::new();
    }
    let mut detail = String::from("\nlinker output:\n");
    if !stderr.trim().is_empty() {
        detail.push_str(&stderr);
    }
    if !stdout.trim().is_empty() {
        detail.push_str(&stdout);
    }
    detail
}
