use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::api::BuildOutputKind;
use crate::error::{AotError, AotResult};

use super::common::format_link_detail;
use super::policy::{append_export_policy_flags, append_external_libraries, append_library_search_paths};
use super::{LinkRequest, LinkResult};

fn windows_import_library_path(shared_library: &Path) -> PathBuf {
    let stem = shared_library.file_stem().and_then(|stem| stem.to_str()).unwrap_or("beskid_runtime");
    shared_library.with_file_name(format!("{stem}_import.lib"))
}

pub(super) fn archive_static_windows(req: &LinkRequest) -> AotResult<LinkResult> {
    let (command_line, output) = run_command_with_fallback(vec![
        windows_lib_command(req, "lib"),
        windows_lib_command(req, "lib.exe"),
        windows_lib_command(req, "llvm-lib"),
        windows_lib_command(req, "llvm-lib.exe"),
    ])?;
    if req.verbose {
        eprintln!("[aot] archive command: {:?}", command_line);
    }
    if !output.status.success() {
        return Err(AotError::LinkFailed {
            status: output.status.code().unwrap_or(-1),
            command: command_line.clone(),
            detail: format_link_detail(&output),
        });
    }
    Ok(LinkResult {
        output_path: req.output_path.clone(),
        command_line,
        exported_symbols: req.exported_symbols.clone(),
    })
}

pub(super) fn link_windows(req: &LinkRequest, target: &str) -> AotResult<LinkResult> {
    let (command_line, output) = run_command_with_fallback(vec![
        windows_link_command(req, target, "link")?,
        windows_link_command(req, target, "link.exe")?,
        windows_link_command(req, target, "lld-link")?,
        windows_link_command(req, target, "lld-link.exe")?,
    ])?;
    if req.verbose {
        eprintln!("[aot] link command: {:?}", command_line);
    }
    if !output.status.success() {
        return Err(AotError::LinkFailed {
            status: output.status.code().unwrap_or(-1),
            command: command_line.clone(),
            detail: format_link_detail(&output),
        });
    }
    Ok(LinkResult {
        output_path: req.output_path.clone(),
        command_line,
        exported_symbols: req.exported_symbols.clone(),
    })
}

fn run_command_with_fallback(commands: Vec<Command>) -> AotResult<(String, std::process::Output)> {
    let mut last_failure: Option<(String, std::process::Output)> = None;

    for mut command in commands {
        let command_line = format!("{:?}", command);
        match command.output() {
            Ok(output) if output.status.success() => return Ok((command_line, output)),
            Ok(output) => {
                last_failure = Some((command_line, output));
            }
            Err(err) if err.kind() == ErrorKind::NotFound => continue,
            Err(_) => return Err(AotError::LinkerUnavailable),
        }
    }

    if let Some((command_line, output)) = last_failure {
        return Err(AotError::LinkFailed {
            status: output.status.code().unwrap_or(-1),
            command: command_line,
            detail: format_link_detail(&output),
        });
    }

    Err(AotError::LinkerUnavailable)
}

fn windows_lib_command(req: &LinkRequest, librarian: &str) -> Command {
    let mut cmd = Command::new(librarian);
    cmd.arg("/NOLOGO");
    cmd.arg(format!("/OUT:{}", req.output_path.display()));
    cmd.arg(&req.object_path);
    cmd.args(&req.additional_object_paths);
    if let Some(runtime_staticlib) = &req.runtime_staticlib {
        cmd.arg(runtime_staticlib);
    }
    if let Some(host_staticlib) = &req.host_staticlib {
        cmd.arg(host_staticlib);
    }
    cmd
}

fn windows_link_command(req: &LinkRequest, target: &str, linker: &str) -> AotResult<Command> {
    let mut cmd = Command::new(linker);
    cmd.arg("/NOLOGO");
    cmd.arg(format!("/OUT:{}", req.output_path.display()));
    if req.output_kind == BuildOutputKind::SharedLib {
        cmd.arg("/DLL");
        cmd.arg("/NOENTRY");
        cmd.arg(format!("/IMPLIB:{}", windows_import_library_path(&req.output_path).display()));
    }
    cmd.arg(&req.object_path);
    cmd.args(&req.additional_object_paths);
    if let Some(runtime_staticlib) = &req.runtime_staticlib {
        cmd.arg(runtime_staticlib);
    }
    if let Some(host_staticlib) = &req.host_staticlib {
        cmd.arg(host_staticlib);
    }
    append_library_search_paths(req, target, &mut cmd)?;
    append_external_libraries(req, target, &mut cmd)?;
    if req.output_kind == BuildOutputKind::SharedLib {
        append_export_policy_flags(req, target, &mut cmd)?;
    }
    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::api::{BuildOutputKind, LinkMode};

    use super::{LinkRequest, windows_import_library_path, windows_link_command};

    #[test]
    fn derives_a_stable_windows_import_library_name_from_the_shared_dll() {
        assert_eq!(
            windows_import_library_path(Path::new("out/beskid_runtime.dll")),
            Path::new("out/beskid_runtime_import.lib")
        );
    }

    #[test]
    fn windows_shared_link_requests_the_named_coff_import_library() {
        let command = windows_link_command(
            &LinkRequest {
                target_triple: Some("x86_64-pc-windows-msvc".into()),
                output_kind: BuildOutputKind::SharedLib,
                output_path: PathBuf::from("out/beskid_runtime.dll"),
                object_path: PathBuf::from("out/runtime.obj"),
                additional_object_paths: vec![PathBuf::from("out/context.obj")],
                runtime_staticlib: None,
                host_staticlib: None,
                entrypoint_symbol: String::new(),
                exported_symbols: vec!["beskid_rt_v5_abi_version".into()],
                link_mode: LinkMode::Auto,
                verbose: false,
                external_libraries: vec!["kernel32".into()],
                library_search_paths: vec![PathBuf::from("sdk/lib")],
            },
            "x86_64-pc-windows-msvc",
            "link",
        )
        .expect("build Windows link command");
        let arguments = command.get_args().map(|argument| argument.to_string_lossy().into_owned()).collect::<Vec<_>>();

        assert_eq!(command.get_program(), "link");
        for required in [
            "/DLL",
            "/NOENTRY",
            "/OUT:out/beskid_runtime.dll",
            "/IMPLIB:out/beskid_runtime_import.lib",
            "/EXPORT:beskid_rt_v5_abi_version",
            "/LIBPATH:sdk/lib",
            "kernel32.lib",
        ] {
            assert!(arguments.iter().any(|argument| argument == required), "missing {required}: {arguments:?}");
        }
        assert!(
            !arguments.iter().any(|argument| argument == "-shared" || argument.starts_with("-Wl,")),
            "Windows link command leaked Unix linker flags: {arguments:?}"
        );
    }
}
