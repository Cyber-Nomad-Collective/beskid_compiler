//! Host linker and static-archive integration (`cc` / `cl`, `ar`, `libtool`, version scripts).

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::api::{BuildOutputKind, LinkMode};
use crate::error::{AotError, AotResult};

/// Arguments for [`link`]: object path, optional runtime archive, output shape, and exports.
#[derive(Debug, Clone)]
pub struct LinkRequest {
    pub target_triple: Option<String>,
    pub output_kind: BuildOutputKind,
    pub output_path: PathBuf,
    pub object_path: PathBuf,
    /// Additional native object files that must be present in every linked/archive artifact.
    pub additional_object_paths: Vec<PathBuf>,
    pub runtime_staticlib: Option<PathBuf>,
    pub host_staticlib: Option<PathBuf>,
    pub entrypoint_symbol: String,
    pub exported_symbols: Vec<String>,
    pub link_mode: LinkMode,
    pub verbose: bool,
    pub external_libraries: Vec<String>,
    pub library_search_paths: Vec<PathBuf>,
}

fn detect_c_compiler() -> String {
    if let Ok(value) = std::env::var("CC") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return trimmed.to_owned();
        }
    }

    if cfg!(target_os = "windows") {
        "cl".to_owned()
    } else {
        "cc".to_owned()
    }
}

fn append_static_archive(cmd: &mut Command, target: &str, archive: &std::path::Path) {
    if target.contains("darwin") || target.contains("macos") {
        cmd.arg("-Wl,-force_load").arg(archive);
    } else {
        cmd.arg(archive);
    }
}

fn format_link_command(compiler: &str, req: &LinkRequest, target: &str) -> String {
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

fn format_link_detail(output: &std::process::Output) -> String {
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

/// Successful link or archive merge: output path, echoed command line, and export list carried through.
#[derive(Debug, Clone)]
pub struct LinkResult {
    pub output_path: PathBuf,
    pub command_line: String,
    pub exported_symbols: Vec<String>,
}

/// Link or merge into `req.output_path` using the host toolchain (see module docs for platform notes).
pub fn link(req: &LinkRequest) -> AotResult<LinkResult> {
    if !req.object_path.exists() {
        return Err(AotError::Io {
            path: req.object_path.clone(),
            message: "object file does not exist".to_owned(),
        });
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
        return Err(AotError::RuntimeArchiveMissing {
            path: runtime_staticlib.clone(),
        });
    }

    if let Some(parent) = req.output_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| AotError::Io {
            path: parent.to_path_buf(),
            message: err.to_string(),
        })?;
    }

    if req.output_kind == BuildOutputKind::StaticLib {
        return archive_static(req);
    }

    if req.output_kind == BuildOutputKind::Exe && req.entrypoint_symbol != "main" {
        return Err(AotError::UnsupportedLinkerStrategy {
            target: req
                .target_triple
                .clone()
                .unwrap_or_else(|| std::env::consts::OS.to_owned()),
            message: "executable output currently requires entrypoint symbol `main`".to_owned(),
        });
    }

    let compiler = detect_c_compiler();
    let target = req
        .target_triple
        .as_deref()
        .unwrap_or(std::env::consts::OS)
        .to_ascii_lowercase();
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

fn archive_static(req: &LinkRequest) -> AotResult<LinkResult> {
    let target = req
        .target_triple
        .as_deref()
        .unwrap_or(std::env::consts::OS)
        .to_ascii_lowercase();

    if target.contains("windows") {
        return archive_static_windows(req);
    }

    // macOS ships BSD `ar`, which does not implement GNU binutils MRI scripts (`ar -M`).
    // Xcode's `libtool -static` is the supported way to merge a static archive with objects.
    let is_apple_host_style =
        target.contains("darwin") || target.contains("apple") || target.contains("macos");
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
                command: format!(
                    "ar crs {} {}",
                    req.output_path.display(),
                    req.object_path.display()
                ),
                detail: format_link_detail(&output),
            });
        }
        return Ok(LinkResult {
            output_path: req.output_path.clone(),
            exported_symbols: req.exported_symbols.clone(),
            command_line: format!(
                "ar crs {} {}",
                req.output_path.display(),
                req.object_path.display()
            ),
        });
    }

    let script_path = req.output_path.with_extension("mri");
    let runtime_lib = req
        .runtime_staticlib
        .as_ref()
        .expect("runtime checked above");
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
    std::fs::write(&script_path, script).map_err(|err| AotError::Io {
        path: script_path.clone(),
        message: err.to_string(),
    })?;

    let mut shell_command = Command::new("sh");
    shell_command
        .arg("-c")
        .arg(format!("ar -M < {}", script_path.to_string_lossy()));

    if req.verbose {
        eprintln!("[aot] archive command: {:?}", shell_command);
    }

    let output = shell_command
        .output()
        .map_err(|_| AotError::LinkerUnavailable)?;
    if !output.status.success() {
        return Err(AotError::LinkFailed {
            status: output.status.code().unwrap_or(-1),
            command: format!("ar -M < {}", script_path.display()),
            detail: format_link_detail(&output),
        });
    }

    let ranlib_out = Command::new("ranlib")
        .arg(&req.output_path)
        .output()
        .map_err(|_| AotError::LinkerUnavailable)?;
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
            "ar -M < {} && ranlib {}",
            script_path.display(),
            req.output_path.display()
        ),
        exported_symbols: req.exported_symbols.clone(),
    })
}

fn windows_import_library_path(shared_library: &Path) -> PathBuf {
    let stem = shared_library
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("beskid_runtime");
    shared_library.with_file_name(format!("{stem}_import.lib"))
}

fn archive_static_windows(req: &LinkRequest) -> AotResult<LinkResult> {
    let mut cmd = Command::new("lib");
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
    if req.verbose {
        eprintln!("[aot] archive command: {:?}", cmd);
    }
    let output = cmd.output().map_err(|_| AotError::LinkerUnavailable)?;
    if !output.status.success() {
        return Err(AotError::LinkFailed {
            status: output.status.code().unwrap_or(-1),
            command: format!("{:?}", cmd),
            detail: format_link_detail(&output),
        });
    }
    Ok(LinkResult {
        output_path: req.output_path.clone(),
        command_line: format!("{:?}", cmd),
        exported_symbols: req.exported_symbols.clone(),
    })
}

fn link_windows(req: &LinkRequest, target: &str) -> AotResult<LinkResult> {
    let mut cmd = windows_link_command(req, target)?;
    if req.verbose {
        eprintln!("[aot] link command: {:?}", cmd);
    }
    let output = cmd.output().map_err(|_| AotError::LinkerUnavailable)?;
    if !output.status.success() {
        return Err(AotError::LinkFailed {
            status: output.status.code().unwrap_or(-1),
            command: format!("{:?}", cmd),
            detail: format_link_detail(&output),
        });
    }
    Ok(LinkResult {
        output_path: req.output_path.clone(),
        command_line: format!("{:?}", cmd),
        exported_symbols: req.exported_symbols.clone(),
    })
}

fn windows_link_command(req: &LinkRequest, target: &str) -> AotResult<Command> {
    let mut cmd = Command::new("link");
    cmd.arg("/NOLOGO");
    cmd.arg(format!("/OUT:{}", req.output_path.display()));
    if req.output_kind == BuildOutputKind::SharedLib {
        cmd.arg("/DLL");
        cmd.arg("/NOENTRY");
        cmd.arg(format!(
            "/IMPLIB:{}",
            windows_import_library_path(&req.output_path).display()
        ));
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

fn archive_static_libtool(req: &LinkRequest) -> AotResult<LinkResult> {
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
                command: format!(
                    "libtool -static -o {} {}",
                    req.output_path.display(),
                    req.object_path.display()
                ),
                detail: format_link_detail(&output),
            });
        }
        return Ok(LinkResult {
            output_path: req.output_path.clone(),
            command_line: format!(
                "libtool -static -o {} {}",
                req.output_path.display(),
                req.object_path.display()
            ),
            exported_symbols: req.exported_symbols.clone(),
        });
    }
    let runtime_lib = req
        .runtime_staticlib
        .as_ref()
        .ok_or_else(|| AotError::InvalidRequest {
            message: "static archive output requires runtime archive unless standalone object-only mode is used"
                .to_owned(),
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

    let ranlib_out = Command::new("ranlib")
        .arg(&req.output_path)
        .output()
        .map_err(|_| AotError::LinkerUnavailable)?;
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

fn append_library_search_paths(
    req: &LinkRequest,
    target: &str,
    cmd: &mut Command,
) -> AotResult<()> {
    if req.library_search_paths.is_empty() {
        return Ok(());
    }
    if target.contains("windows") {
        for path in &req.library_search_paths {
            cmd.arg(format!("/LIBPATH:{}", path.display()));
        }
        return Ok(());
    }
    for path in &req.library_search_paths {
        cmd.arg(format!("-L{}", path.display()));
    }
    Ok(())
}

fn append_external_libraries(req: &LinkRequest, target: &str, cmd: &mut Command) -> AotResult<()> {
    if req.external_libraries.is_empty() {
        return Ok(());
    }
    if target.contains("windows") {
        for library in &req.external_libraries {
            cmd.arg(format!("{}.lib", library.trim()));
        }
        return Ok(());
    }
    for library in &req.external_libraries {
        let name = library.trim();
        if name.is_empty() {
            continue;
        }
        if name.starts_with("-l") {
            cmd.arg(name);
        } else {
            cmd.arg(format!("-l{name}"));
        }
    }
    Ok(())
}

fn append_export_policy_flags(req: &LinkRequest, target: &str, cmd: &mut Command) -> AotResult<()> {
    if req.exported_symbols.is_empty() {
        return Ok(());
    }

    if target.contains("linux") || target.contains("gnu") || target.contains("musl") {
        let script_path = req.output_path.with_extension("exports.map");
        let mut script = String::from("{\n  global:\n");
        for symbol in &req.exported_symbols {
            script.push_str(&format!("    {symbol};\n"));
        }
        script.push_str("  local: *;\n};\n");
        std::fs::write(&script_path, script).map_err(|err| AotError::Io {
            path: script_path.clone(),
            message: err.to_string(),
        })?;
        cmd.arg(format!("-Wl,--version-script={}", script_path.display()));
        return Ok(());
    }

    if target.contains("darwin") || target.contains("apple") || target.contains("macos") {
        for symbol in &req.exported_symbols {
            cmd.arg(format!("-Wl,-exported_symbol,_{}", symbol));
        }
        return Ok(());
    }

    if target.contains("windows") {
        for symbol in &req.exported_symbols {
            cmd.arg(format!("/EXPORT:{symbol}"));
        }
        return Ok(());
    }

    Err(AotError::UnsupportedLinkerStrategy {
        target: target.to_owned(),
        message: "shared export policy flags are not implemented for this target".to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{
        BuildOutputKind, LinkMode, LinkRequest, windows_import_library_path, windows_link_command,
    };

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
        )
        .expect("build Windows link command");
        let arguments = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

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
            assert!(
                arguments.iter().any(|argument| argument == required),
                "missing {required}: {arguments:?}"
            );
        }
        assert!(
            !arguments
                .iter()
                .any(|argument| argument == "-shared" || argument.starts_with("-Wl,")),
            "Windows link command leaked Unix linker flags: {arguments:?}"
        );
    }
}
