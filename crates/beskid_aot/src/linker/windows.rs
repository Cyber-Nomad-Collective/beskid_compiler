use std::path::{Path, PathBuf};
use std::process::Command;

use crate::api::BuildOutputKind;
use crate::error::{AotError, AotResult};

use super::common::format_link_detail;
use super::policy::{
    append_export_policy_flags, append_external_libraries, append_library_search_paths, append_optional_libraries,
};
use super::{LinkRequest, LinkResult};

fn windows_import_library_path(shared_library: &Path) -> PathBuf {
    let stem = shared_library.file_stem().and_then(|stem| stem.to_str()).unwrap_or("beskid_runtime");
    shared_library.with_file_name(format!("{stem}_import.lib"))
}

pub(super) fn archive_static_windows(req: &LinkRequest) -> AotResult<LinkResult> {
    if let Some(runtime) = &req.runtime {
        runtime.static_archive()?;
    }
    let (command_line, output, tool_receipt) = run_command_with_fallback(
        vec![
            windows_lib_command(req, "lib"),
            windows_lib_command(req, "lib.exe"),
            windows_lib_command(req, "llvm-lib"),
            windows_lib_command(req, "llvm-lib.exe"),
        ],
        None,
    )?;
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
        tool_receipt: Some(tool_receipt),
        output_path: req.output_path.clone(),
        command_line,
        exported_symbols: req.exported_symbols.clone(),
    })
}

pub(super) fn link_windows(
    req: &LinkRequest,
    target: &str,
    control: Option<&crate::api::NativeExecutionControl>,
) -> AotResult<LinkResult> {
    let (command_line, output, tool_receipt) = run_command_with_fallback(
        vec![
            windows_link_command(req, target, "link")?,
            windows_link_command(req, target, "link.exe")?,
            windows_link_command(req, target, "lld-link")?,
            windows_link_command(req, target, "lld-link.exe")?,
        ],
        control,
    )?;
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
    place_shared_provider_beside_executable(req)?;
    Ok(LinkResult {
        tool_receipt: Some(tool_receipt),
        output_path: req.output_path.clone(),
        command_line,
        exported_symbols: req.exported_symbols.clone(),
    })
}

/// PE/COFF has no run path. An executable linked against the canonical shared provider imports it
/// by its canonical DLL basename, and the Windows loader resolves that name through its standard
/// search order, which starts with the directory the application was loaded from
/// (<https://learn.microsoft.com/windows/win32/dlls/dynamic-link-library-search-order>). The ELF and
/// Mach-O links record the provider's installed directory as a run path instead; on Windows the
/// exact validated provider is published beside the executable, so the program loads it without
/// `PATH` changes. The copy is staged in the output directory and renamed into place; an identical
/// provider already there is kept, and any failure fails the link.
pub(super) fn place_shared_provider_beside_executable(req: &LinkRequest) -> AotResult<()> {
    let Some(super::RuntimeLinkInput::GlueSharedProviderV1 { shared_library_path, .. }) = &req.runtime else {
        return Ok(());
    };
    if req.output_kind != BuildOutputKind::Exe {
        return Ok(());
    }
    let io_error =
        |path: &Path, error: std::io::Error| AotError::Io { path: path.to_owned(), message: error.to_string() };
    let name = shared_library_path.file_name().ok_or_else(|| AotError::InvalidRequest {
        message: format!("shared runtime provider `{}` has no file name", shared_library_path.display()),
    })?;
    let directory = req.output_path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let destination = directory.join(name);
    if destination == *shared_library_path {
        return Ok(());
    }
    let provider = std::fs::read(shared_library_path).map_err(|error| io_error(shared_library_path, error))?;
    match std::fs::read(&destination) {
        Ok(existing) if existing == provider => return Ok(()),
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_error(&destination, error)),
    }
    let mut staged = tempfile::Builder::new()
        .prefix(".beskid-provider-")
        .tempfile_in(directory)
        .map_err(|error| io_error(directory, error))?;
    std::io::Write::write_all(&mut staged, &provider).map_err(|error| io_error(staged.path(), error))?;
    staged.as_file().sync_all().map_err(|error| io_error(staged.path(), error))?;
    // `persist` clears the temporary-file attribute before it renames over the destination.
    staged.persist(&destination).map_err(|error| io_error(&destination, error.error))?;
    Ok(())
}

fn run_command_with_fallback(
    commands: Vec<Command>,
    control: Option<&crate::api::NativeExecutionControl>,
) -> AotResult<(String, std::process::Output, super::LinkToolReceipt)> {
    let mut last_failure: Option<(String, std::process::Output)> = None;

    for mut command in commands {
        crate::windows_toolchain::configure_windows_native_command_with_control(&mut command, control)?;
        let command_line = format!("{:?}", command);
        let output = super::run_link_tool(
            &super::LinkToolInvocation::from_link_command(&command),
            std::env::temp_dir().as_path(),
            control,
        );
        match output {
            Ok((output, receipt)) if output.status.success() => return Ok((command_line, output, receipt)),
            Ok((output, _)) => {
                last_failure = Some((command_line, output));
            }
            Err(AotError::LinkerUnavailable) => continue,
            Err(error) => return Err(AotError::InvalidRequest { message: format!("native windows link: {error}") }),
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
    if let Some(runtime) = &req.runtime {
        cmd.arg(runtime.path());
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
    } else if req.output_kind == BuildOutputKind::Exe {
        cmd.arg("/SUBSYSTEM:CONSOLE");
        // The native runtime deliberately shares the application's dynamic UCRT. Linking the
        // console import library gives the linker its standard `main`/`wmain` startup thunk
        // without reintroducing a raw PE entrypoint or a static CRT into the runtime archive.
        cmd.arg("msvcrt.lib");
    }
    cmd.arg(&req.object_path);
    cmd.args(&req.additional_object_paths);
    if let Some(runtime) = &req.runtime {
        cmd.arg(runtime.path());
    }
    if let Some(host_staticlib) = &req.host_staticlib {
        cmd.arg(host_staticlib);
    }
    append_library_search_paths(req, target, &mut cmd)?;
    append_external_libraries(req, target, &mut cmd)?;
    append_optional_libraries(req, target, linker, &mut cmd)?;
    if req.output_kind == BuildOutputKind::SharedLib {
        append_export_policy_flags(req, target, &mut cmd)?;
    }
    Ok(cmd)
}

#[cfg(test)]
mod tests {
    #[test]
    fn optional_extern_libraries_fail_closed_on_windows() {
        let request = LinkRequest {
            target_triple: Some("x86_64-pc-windows-msvc".into()),
            output_kind: BuildOutputKind::Exe,
            output_path: "app.exe".into(),
            object_path: "app.obj".into(),
            additional_object_paths: Vec::new(),
            runtime: None,
            host_staticlib: None,
            entrypoint_symbol: "main".into(),
            exported_symbols: Vec::new(),
            link_mode: crate::api::LinkMode::Auto,
            verbose: false,
            external_libraries: Vec::new(),
            optional_libraries: vec![crate::linker::OptionalLinkLibrary {
                library: "libcrypto-3-x64.dll".into(),
                symbols: vec!["SHA256".into()],
            }],
            library_search_paths: Vec::new(),
        };
        let error = windows_link_command(&request, "x86_64-pc-windows-msvc", "link.exe")
            .expect_err("optional contracts are rejected for Windows");
        assert!(error.to_string().contains("optional [Extern]"), "{error}");
    }

    use std::path::{Path, PathBuf};

    use crate::api::{BuildOutputKind, LinkMode};

    use super::{
        LinkRequest, place_shared_provider_beside_executable, windows_import_library_path, windows_link_command,
    };

    fn provider_request(output_kind: BuildOutputKind, output_path: PathBuf, provider: PathBuf) -> LinkRequest {
        LinkRequest {
            target_triple: Some("x86_64-pc-windows-msvc".into()),
            output_kind,
            output_path,
            object_path: "app.obj".into(),
            additional_object_paths: Vec::new(),
            runtime: Some(crate::linker::RuntimeLinkInput::GlueSharedProviderV1 {
                link_path: provider.with_file_name("beskid_runtime_import.lib"),
                shared_library_path: provider,
            }),
            host_staticlib: None,
            entrypoint_symbol: "main".into(),
            exported_symbols: Vec::new(),
            link_mode: LinkMode::Auto,
            verbose: false,
            external_libraries: Vec::new(),
            optional_libraries: Vec::new(),
            library_search_paths: Vec::new(),
        }
    }

    /// Windows regression (v0.6.0 release DYN-06-01..05): every native test executable that links
    /// the shared provider exited STATUS_DLL_NOT_FOUND, because PE has no run path and the provider
    /// stayed only in the runtime kit.
    #[test]
    fn windows_executable_with_shared_provider_gets_the_exact_provider_beside_it() {
        let temp = tempfile::tempdir().unwrap();
        let kit = temp.path().join("kit/shared");
        std::fs::create_dir_all(&kit).unwrap();
        let provider = kit.join("beskid_runtime.dll");
        std::fs::write(&provider, b"canonical provider bytes").unwrap();
        let output = temp.path().join("entry-0");
        std::fs::create_dir_all(&output).unwrap();
        let placed = output.join("beskid_runtime.dll");

        place_shared_provider_beside_executable(&provider_request(
            BuildOutputKind::Exe,
            output.join("test.exe"),
            provider.clone(),
        ))
        .unwrap();
        assert_eq!(std::fs::read(&placed).unwrap(), b"canonical provider bytes");

        // A stale provider from another kit is replaced by the one this executable linked.
        std::fs::write(&placed, b"stale provider").unwrap();
        place_shared_provider_beside_executable(&provider_request(
            BuildOutputKind::Exe,
            output.join("test.exe"),
            provider.clone(),
        ))
        .unwrap();
        assert_eq!(std::fs::read(&placed).unwrap(), b"canonical provider bytes");
        let leftovers = std::fs::read_dir(&output)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(".beskid-provider-"))
            .collect::<Vec<_>>();
        assert!(leftovers.is_empty(), "staging files left behind: {leftovers:?}");

        // Shared libraries are loaded by a host that already loaded the provider.
        let library = temp.path().join("lib");
        std::fs::create_dir_all(&library).unwrap();
        place_shared_provider_beside_executable(&provider_request(
            BuildOutputKind::SharedLib,
            library.join("mod.dll"),
            provider,
        ))
        .unwrap();
        assert!(!library.join("beskid_runtime.dll").exists());
    }

    #[test]
    fn derives_a_stable_windows_import_library_name_from_the_shared_dll() {
        assert_eq!(
            windows_import_library_path(Path::new("out/beskid_runtime.dll")),
            Path::new("out/beskid_runtime_import.lib")
        );
    }

    #[test]
    fn windows_shared_link_requests_the_named_coff_import_library() {
        let temp = tempfile::tempdir().expect("temp dir for windows link command");
        let output_path = temp.path().join("beskid_runtime.dll");
        let command = windows_link_command(
            &LinkRequest {
                target_triple: Some("x86_64-pc-windows-msvc".into()),
                output_kind: BuildOutputKind::SharedLib,
                output_path: output_path.clone(),
                object_path: PathBuf::from("out/runtime.obj"),
                additional_object_paths: vec![PathBuf::from("out/context.obj")],
                runtime: None,
                host_staticlib: None,
                entrypoint_symbol: String::new(),
                exported_symbols: vec!["beskid_rt_v5_abi_version".into()],
                link_mode: LinkMode::Auto,
                verbose: false,
                external_libraries: vec!["kernel32".into()],
                optional_libraries: Vec::new(),
                library_search_paths: vec![PathBuf::from("sdk/lib")],
            },
            "x86_64-pc-windows-msvc",
            "link",
        )
        .expect("build Windows link command");
        let arguments = command.get_args().map(|argument| argument.to_string_lossy().into_owned()).collect::<Vec<_>>();

        assert_eq!(command.get_program(), "link");
        let expected_def = format!("/DEF:{}", output_path.with_extension("exports.def").display());
        let required = [
            "/DLL".to_owned(),
            "/NOENTRY".to_owned(),
            format!("/OUT:{}", output_path.display()),
            format!("/IMPLIB:{}", windows_import_library_path(&output_path).display()),
            expected_def,
            "/LIBPATH:sdk/lib".to_owned(),
            "kernel32.lib".to_owned(),
        ];
        for required in required.iter() {
            assert!(arguments.iter().any(|argument| argument == required), "missing {required}: {arguments:?}");
        }
        assert!(
            !arguments.iter().any(|argument| argument == "-shared" || argument.starts_with("-Wl,")),
            "Windows link command leaked Unix linker flags: {arguments:?}"
        );
        let def_path = output_path.with_extension("exports.def");
        let def_contents = std::fs::read_to_string(&def_path).expect("def file written");
        assert!(def_contents.contains("EXPORTS"), "def file missing EXPORTS section: {def_contents}");
        assert!(def_contents.contains("beskid_rt_v5_abi_version"), "def file missing export: {def_contents}");
    }

    #[test]
    fn windows_executable_lets_the_crt_select_console_startup() {
        let command = windows_link_command(
            &LinkRequest {
                target_triple: Some("x86_64-pc-windows-msvc".into()),
                output_kind: BuildOutputKind::Exe,
                output_path: PathBuf::from("out/beskid_run.exe"),
                object_path: PathBuf::from("out/main.obj"),
                additional_object_paths: Vec::new(),
                runtime: Some(crate::linker::RuntimeLinkInput::CanonicalStatic {
                    archive: PathBuf::from("runtime/beskid_runtime.lib"),
                }),
                host_staticlib: None,
                entrypoint_symbol: "main".into(),
                exported_symbols: vec!["main".into()],
                link_mode: LinkMode::Auto,
                verbose: false,
                external_libraries: vec!["kernel32".into()],
                optional_libraries: Vec::new(),
                library_search_paths: Vec::new(),
            },
            "x86_64-pc-windows-msvc",
            "lld-link",
        )
        .expect("build Windows executable link command");
        let arguments = command.get_args().map(|argument| argument.to_string_lossy().into_owned()).collect::<Vec<_>>();

        for required in ["/SUBSYSTEM:CONSOLE", "msvcrt.lib"] {
            assert!(arguments.iter().any(|argument| argument == required), "missing {required}: {arguments:?}");
        }
        assert!(
            !arguments.iter().any(|argument| argument.starts_with("/ENTRY:")),
            "Windows console executables must leave startup selection to the CRT: {arguments:?}"
        );
    }
}
