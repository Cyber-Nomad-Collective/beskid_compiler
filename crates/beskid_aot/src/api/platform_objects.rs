use std::path::PathBuf;
use std::process::Command;

use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata, render_runtime_asm_include};
use beskid_abi::generated::abi_v5_contract::GeneratedCoreArgsEntryAdapter;
use cargo_cross::config::{Arch, Os, get_target_config};

use crate::error::{AotError, AotResult};
use crate::windows_toolchain::configure_windows_native_command;

fn tool_unavailable(tool: impl AsRef<std::ffi::OsStr>, error: std::io::Error) -> AotError {
    let tool = tool.as_ref().to_string_lossy().into_owned();
    let mut message = error.to_string();
    if tool == "cl" {
        message.push_str(
            "; install the Visual Studio Build Tools `Desktop development with C++` workload (MSVC x64 and a \
             Windows SDK); Beskid also requires a complete VS 2022 installation discoverable by vswhere.exe",
        );
    }
    AotError::NativeToolUnavailable { tool, message }
}

/// Bootstrap C sources that every linked executable compiles, relative to the `beskid_abi` crate.
///
/// They are embedded so an installed compiler never reads its build machine's source tree: the
/// `CARGO_MANIFEST_DIR` of a release build does not exist on an end-user machine.
const EMBEDDED_BOOTSTRAP_SOURCES: &[(&str, &str)] = &[
    (
        "assembly/common/executable_bootstrap.c",
        include_str!("../../../beskid_abi/assembly/common/executable_bootstrap.c"),
    ),
    ("include/beskid_runtime_abi_v5.h", include_str!("../../../beskid_abi/include/beskid_runtime_abi_v5.h")),
];

/// Write the embedded bootstrap sources below `output_dir` and return their `assembly` root.
///
/// The layout mirrors `beskid_abi` so the sources' relative `#include` lines and the generated
/// Core.Args `entry_source` paths (relative to `assembly/<target>/`) resolve unchanged.
fn materialize_bootstrap_sources(output_dir: &std::path::Path, target: &str) -> AotResult<PathBuf> {
    let root = output_dir.join("beskid-bootstrap");
    for (relative, contents) in EMBEDDED_BOOTSTRAP_SOURCES {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| AotError::Io { path: parent.to_path_buf(), message: err.to_string() })?;
        }
        std::fs::write(&path, contents).map_err(|err| AotError::Io { path: path.clone(), message: err.to_string() })?;
    }
    let target_root = root.join("assembly").join(target);
    std::fs::create_dir_all(&target_root)
        .map_err(|err| AotError::Io { path: target_root.clone(), message: err.to_string() })?;
    Ok(root.join("assembly"))
}

pub(super) fn compile_context_assembly(
    target: &TargetMetadata,
    output_dir: &std::path::Path,
    name: &str,
) -> AotResult<PathBuf> {
    let assembly_root = beskid_abi::assembly_sources::stage_into(output_dir)
        .map_err(|err| AotError::Io { path: output_dir.to_path_buf(), message: err.to_string() })?
        .join(target.triple.as_str());
    let source =
        assembly_root.join(if target.triple.as_str().contains("windows") { "context.asm" } else { "context.S" });
    let include = output_dir.join(format!("beskid_runtime_abi_v5_{}.inc", target.triple.as_str().replace('-', "_")));
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let rendered = render_runtime_asm_include(&manifest)
        .map_err(|err| AotError::InvalidRequest { message: format!("{err:?}") })?;
    std::fs::write(&include, rendered)
        .map_err(|err| AotError::Io { path: include.clone(), message: err.to_string() })?;
    let object = output_dir
        .join(format!("{name}.context.{}", if target.triple.as_str().contains("windows") { "obj" } else { "o" }));

    let mut command =
        if target.triple.as_str().contains("windows") { Command::new("llvm-ml") } else { Command::new("clang") };
    if target.triple.as_str() == "x86_64-unknown-linux-gnu" {
        command.args(["-target", "x86_64-unknown-linux-gnu", "-c"]);
        command.arg(&source).arg("-I").arg(output_dir).arg("-o").arg(&object);
    } else if target.triple.as_str() == "aarch64-apple-darwin" {
        command.args(["-c", "-arch", "arm64"]);
        command.arg(&source).arg("-I").arg(output_dir).arg("-o").arg(&object);
    } else if target.triple.as_str() == "x86_64-pc-windows-msvc" {
        command.args(["--m64", "/c", "/X", "/Fo"]);
        command.arg(&object).arg("/I").arg(output_dir).arg(&source);
    } else {
        return Err(AotError::UnsupportedLinkerStrategy {
            target: target.triple.as_str().to_owned(),
            message: "no canonical context assembly invocation for target".to_owned(),
        });
    }
    if target.triple.as_str().contains("windows") {
        configure_windows_native_command(&mut command)?;
    }
    let output = command.output().map_err(|error| tool_unavailable(command.get_program(), error))?;
    if !output.status.success() {
        return Err(AotError::LinkFailed {
            status: output.status.code().unwrap_or(-1),
            command: format!("{:?}", command),
            detail: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(object)
}
pub(super) struct PlatformObjects {
    pub paths: Vec<PathBuf>,
    pub issuer_compiler_receipt: crate::linker::LinkToolReceipt,
}

pub(super) fn compile_platform_objects(
    target: &TargetMetadata,
    output_dir: &std::path::Path,
    name: &str,
) -> AotResult<PlatformObjects> {
    let plan = platform_object_plan(target.triple.as_str())?;
    let assembly_root = beskid_abi::assembly_sources::stage_into(output_dir)
        .map_err(|err| AotError::Io { path: output_dir.to_path_buf(), message: err.to_string() })?
        .join(target.triple.as_str());
    let source = assembly_root.join(plan.assembly_source);
    let tls_source = assembly_root.join(plan.tls_source);
    let adapter_source = assembly_root.join(plan.adapter_source);
    let object = output_dir.join(format!("{name}.platform.{}", plan.object_extension));
    let tls_object = output_dir.join(format!("{name}.platform_tls.{}", plan.object_extension));
    let adapter_object = output_dir.join(format!("{name}.platform_host.{}", plan.object_extension));
    let mut assembly = Command::new(plan.assembly_program);
    assembly.args(&plan.assembly_args);
    if plan.assembly_output_before_source {
        assembly.arg(&object).arg(&source);
    } else {
        assembly.arg(&source).arg("-o").arg(&object);
    }
    if target.triple.as_str().contains("windows") {
        configure_windows_native_command(&mut assembly)?;
    }
    let output = assembly.output().map_err(|error| tool_unavailable(plan.assembly_program, error))?;
    if !output.status.success() {
        return Err(AotError::LinkFailed {
            status: output.status.code().unwrap_or(-1),
            command: format!(
                "{} {:?} {} -o {}",
                plan.assembly_program,
                plan.assembly_args,
                source.display(),
                object.display()
            ),
            detail: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    let mut tls_command = Command::new(plan.tls_program);
    tls_command.args(&plan.tls_args).arg(&tls_source).arg("-o").arg(&tls_object);
    if target.triple.as_str().contains("windows") {
        configure_windows_native_command(&mut tls_command)?;
    }
    let output = tls_command.output().map_err(|error| tool_unavailable(plan.tls_program, error))?;
    if !output.status.success() {
        return Err(AotError::LinkFailed {
            status: output.status.code().unwrap_or(-1),
            command: format!(
                "{} {:?} {} -o {}",
                plan.tls_program,
                plan.tls_args,
                tls_source.display(),
                tls_object.display()
            ),
            detail: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    let mut adapter_command = Command::new(plan.tls_program);
    adapter_command.args(&plan.tls_args).arg(&adapter_source).arg("-o").arg(&adapter_object);
    if target.triple.as_str().contains("windows") {
        configure_windows_native_command(&mut adapter_command)?;
    }
    let output = adapter_command.output().map_err(|error| tool_unavailable(plan.tls_program, error))?;
    if !output.status.success() {
        return Err(AotError::LinkFailed {
            status: output.status.code().unwrap_or(-1),
            command: format!(
                "{} {:?} {} -o {}",
                plan.tls_program,
                plan.tls_args,
                adapter_source.display(),
                adapter_object.display()
            ),
            detail: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    // Compile exactly one process issuer object into the canonical runtime provider.
    // User DLLs must link that shared provider; they never compile this source.
    let issuer_source = output_dir.join("owner_identity_v1.c");
    let issuer_header = output_dir.join("owner_identity_v1.h");
    for (path, contents) in [
        (&issuer_source, include_str!("../../../../runtime/Glue/owner_identity_v1.c")),
        (&issuer_header, include_str!("../../../../runtime/Glue/owner_identity_v1.h")),
    ] {
        std::fs::write(path, contents).map_err(|err| AotError::Io { path: path.clone(), message: err.to_string() })?;
    }
    let issuer_object = output_dir.join(format!("{name}.glue_owner_issuer_v1.{}", plan.object_extension));
    let mut issuer_command = Command::new(plan.tls_program);
    issuer_command.args(&plan.tls_args).arg(&issuer_source).arg("-o").arg(&issuer_object);
    if target.triple.as_str().contains("windows") {
        configure_windows_native_command(&mut issuer_command)?;
    }
    let invocation = crate::linker::LinkToolInvocation {
        program: issuer_command.get_program().to_os_string(),
        args: issuer_command.get_args().map(|arg| arg.to_os_string()).collect(),
        environment: issuer_command
            .get_envs()
            .map(|(key, value)| (key.to_os_string(), value.map(|v| v.to_os_string())))
            .collect(),
        current_dir: None,
    };
    let control = super::NativeExecutionControl::new(
        std::time::Instant::now() + std::time::Duration::from_secs(300),
        std::sync::Arc::new(|| false),
    );
    let (output, issuer_compiler_receipt) = crate::linker::run_link_tool(&invocation, output_dir, Some(&control))?;
    if !output.status.success() {
        return Err(AotError::LinkFailed {
            status: output.status.code().unwrap_or(-1),
            command: format!("{issuer_command:?}"),
            detail: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(PlatformObjects { paths: vec![object, tls_object, adapter_object, issuer_object], issuer_compiler_receipt })
}

pub(super) fn compile_executable_bootstrap(
    target: &str,
    core_args: Option<&GeneratedCoreArgsEntryAdapter>,
    output_dir: &std::path::Path,
    name: &str,
    program_returns_void: bool,
    initialization: Option<(&str, u64)>,
) -> AotResult<PathBuf> {
    let assembly_root = materialize_bootstrap_sources(output_dir, target)?;
    let source = executable_bootstrap_source(&assembly_root, core_args);
    if !source.is_file() {
        return Err(AotError::InvalidRequest {
            message: format!("executable bootstrap source `{}` is not embedded in this compiler", source.display()),
        });
    }
    let (mut command, object) =
        executable_bootstrap_command(target, core_args, &assembly_root, output_dir, name, program_returns_void)?;
    configure_dynamic_initialization(&mut command, initialization)?;
    if target.contains("windows") {
        configure_windows_native_command(&mut command)?;
    }
    let output = command.output().map_err(|error| tool_unavailable(command.get_program(), error))?;
    if !output.status.success() {
        return Err(AotError::LinkFailed {
            status: output.status.code().unwrap_or(-1),
            command: format!("{:?}", command),
            detail: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(object)
}

fn executable_bootstrap_command(
    target: &str,
    core_args: Option<&GeneratedCoreArgsEntryAdapter>,
    assembly_root: &std::path::Path,
    output_dir: &std::path::Path,
    name: &str,
    program_returns_void: bool,
) -> AotResult<(Command, PathBuf)> {
    let plan = platform_object_plan(target)?;
    let source = executable_bootstrap_source(assembly_root, core_args);
    let object = output_dir.join(format!("{name}.executable_bootstrap.{}", plan.object_extension));
    let windows = target.contains("windows");
    let mut command = if windows { Command::new("cl") } else { Command::new(plan.tls_program) };
    if windows {
        command.args(["/nologo", "/std:c11", "/MD", "/c"]);
    } else {
        command.args(&plan.tls_args);
    }
    if program_returns_void {
        command.arg("-DBESKID_EXECUTABLE_PROGRAM_RETURNS_VOID");
    }
    if let Some(adapter) = core_args {
        match adapter.capture {
            "utf8_argv" => {
                command.arg("-DBESKID_EXECUTABLE_CORE_ARGS_UTF8");
            }
            "utf16_wargv" => {
                command.arg("-DBESKID_EXECUTABLE_CORE_ARGS_UTF16");
            }
            capture => {
                return Err(AotError::InvalidRequest {
                    message: format!("Core.Args adapter `{}` has unsupported capture `{capture}`", adapter.target),
                });
            }
        }
    }
    if windows {
        command.arg(format!("/Fo{}", msvc_compatible_path(&object))).arg(msvc_compatible_path(&source));
    } else {
        command.arg(&source).arg("-o").arg(&object);
    }
    Ok((command, object))
}

/// MSVC `cl.exe` does not accept Windows verbatim (`\\?\`) paths on its
/// command line, even though Rust's canonicalized project paths use them.
/// Preserve the original PathBuf for filesystem operations and normalize only
/// the argument passed to this native tool.
fn msvc_compatible_path(path: &std::path::Path) -> String {
    let raw = path.to_string_lossy();
    if let Some(rest) = raw.strip_prefix(r"\\?\") {
        let bytes = rest.as_bytes();
        if bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\' {
            return rest.to_owned();
        }
        if let Some(unc) = rest.strip_prefix(r"UNC\") {
            return format!(r"\\{unc}");
        }
    }
    raw.into_owned()
}

/// Resolve the bootstrap source from the generated Core.Args provenance when it is active.
/// Non-argument executables use the same common source directly.
fn executable_bootstrap_source(
    assembly_root: &std::path::Path,
    core_args: Option<&GeneratedCoreArgsEntryAdapter>,
) -> PathBuf {
    core_args
        .map(|adapter| assembly_root.join(adapter.target).join(adapter.entry_source))
        .unwrap_or_else(|| assembly_root.join("common/executable_bootstrap.c"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PlatformObjectPlan {
    assembly_source: &'static str,
    tls_source: &'static str,
    adapter_source: &'static str,
    assembly_program: &'static str,
    assembly_args: Vec<String>,
    assembly_output_before_source: bool,
    tls_program: &'static str,
    tls_args: Vec<String>,
    object_extension: &'static str,
}

fn platform_object_plan(target: &str) -> AotResult<PlatformObjectPlan> {
    // Try cargo_cross config first; fall back to string-based matching for targets
    // not in cargo_cross's database (e.g. msvc variants).
    if let Some(config) = get_target_config(target) {
        return match (&config.arch, &config.os) {
            (Arch::Aarch64, Os::Darwin) => Ok(PlatformObjectPlan {
                assembly_source: "platform.S",
                tls_source: "platform_tls.c",
                adapter_source: "platform_host.c",
                assembly_program: "clang",
                assembly_args: vec!["-c".into(), "-arch".into(), "arm64".into()],
                assembly_output_before_source: false,
                tls_program: "clang",
                tls_args: vec!["-std=c11".into(), "-c".into(), "-arch".into(), "arm64".into()],
                object_extension: "o",
            }),
            (Arch::X86_64, Os::Linux) => Ok(PlatformObjectPlan {
                assembly_source: "platform.S",
                tls_source: "platform_tls.c",
                adapter_source: "platform_host.c",
                assembly_program: "clang",
                assembly_args: vec!["-target".into(), target.to_owned(), "-fPIC".into(), "-c".into()],
                assembly_output_before_source: false,
                tls_program: "clang",
                tls_args: vec!["-target".into(), target.to_owned(), "-std=c11".into(), "-fPIC".into(), "-c".into()],
                object_extension: "o",
            }),
            (Arch::X86_64, Os::Windows) => Ok(PlatformObjectPlan {
                assembly_source: "platform.asm",
                tls_source: "platform_tls.c",
                adapter_source: "platform_host.c",
                assembly_program: "llvm-ml",
                assembly_args: vec!["--m64".into(), "/c".into(), "/X".into(), "/Fo".into()],
                assembly_output_before_source: true,
                tls_program: "clang",
                tls_args: vec![format!("--target={target}"), "-std=c11".into(), "-c".into()],
                object_extension: "obj",
            }),
            _ => Err(AotError::UnsupportedLinkerStrategy {
                target: target.to_owned(),
                message: format!(
                    "native platform shim is not implemented for {}-{}",
                    config.arch.as_str(),
                    config.os.as_str()
                ),
            }),
        };
    }

    // Fallback: string-based target matching for targets not in cargo_cross config DB
    match target {
        "x86_64-pc-windows-msvc" => Ok(PlatformObjectPlan {
            assembly_source: "platform.asm",
            tls_source: "platform_tls.c",
            adapter_source: "platform_host.c",
            assembly_program: "llvm-ml",
            assembly_args: vec!["--m64".into(), "/c".into(), "/X".into(), "/Fo".into()],
            assembly_output_before_source: true,
            tls_program: "clang",
            tls_args: vec!["--target=x86_64-pc-windows-msvc".into(), "-std=c11".into(), "-c".into()],
            object_extension: "obj",
        }),
        _ => Err(AotError::UnsupportedLinkerStrategy {
            target: target.to_owned(),
            message: "native platform shim is not implemented for this host target".to_owned(),
        }),
    }
}

#[cfg(test)]
mod platform_object_tests {
    use std::path::Path;

    use beskid_abi::generated::abi_v5_contract::GeneratedCoreArgsEntryAdapter;

    use super::{executable_bootstrap_command, executable_bootstrap_source, platform_object_plan};

    #[test]
    fn windows_executable_bootstrap_selects_the_dynamic_crt_at_compilation() {
        let scratch = tempfile::tempdir().expect("scratch dir");
        let (command, _) = executable_bootstrap_command(
            "x86_64-pc-windows-msvc",
            None,
            &scratch.path().join("beskid-bootstrap/assembly"),
            scratch.path(),
            "app",
            false,
        )
        .expect("Windows bootstrap command");
        assert_eq!(command.get_program(), "cl");
        let args = command.get_args().collect::<Vec<_>>();
        assert!(args.iter().any(|arg| *arg == "/MD"), "bootstrap must select dynamic CRT defaults: {command:?}");
        assert!(!args.iter().any(|arg| *arg == "/MT" || *arg == "/MDd"));
    }

    #[test]
    fn windows_executable_bootstrap_does_not_pass_verbatim_drive_paths_to_msvc() {
        let (command, _) = executable_bootstrap_command(
            "x86_64-pc-windows-msvc",
            None,
            Path::new(r"\\?\C:\project\obj\beskid-bootstrap\assembly"),
            Path::new(r"\\?\C:\project\obj"),
            "app",
            false,
        )
        .expect("Windows bootstrap command");
        let args = command.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect::<Vec<_>>();
        assert!(args.iter().any(|arg| arg.contains(r"C:\project\obj\beskid-bootstrap\assembly")));
        assert!(!args.iter().any(|arg| arg.contains(r"\\?\")), "MSVC cannot open verbatim paths: {command:?}");
    }

    #[test]
    fn unit_executable_host_initializes_zeroed_state_and_shuts_down_after_program_return() {
        let target = crate::target::detect_target(None).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let bootstrap =
            super::compile_executable_bootstrap(&target.triple, None, temp.path(), "unit", true, None).unwrap();
        let witness = temp.path().join("lifecycle.c");
        std::fs::write(
            &witness,
            r#"
#include "beskid_runtime_abi_v5.h"
#include <stdlib.h>
#include <stdio.h>
static int phase;
static void *observed_state;
void *beskid_rt_v5_process_init(void *state) {
    if ((uintptr_t)state % BESKID_RUNTIME_STATE_ALIGNMENT != 0) _Exit(81);
    for (size_t i = 0; i < BESKID_RUNTIME_STATE_SIZE; ++i)
        if (((unsigned char *)state)[i] != 0) _Exit(82);
    observed_state = state;
    phase = 1;
    return state;
}
void beskid_program_main(void) {
    if (phase != 1) _Exit(83);
    phase = 2;
}
void beskid_rt_v5_process_shutdown(void *state) {
    if (phase != 2 || state != observed_state) _Exit(84);
    phase = 3;
    fputs("shutdown-after-unit-return", stdout);
}
"#,
        )
        .unwrap();
        let include = Path::new(env!("CARGO_MANIFEST_DIR")).join("../beskid_abi/include");
        let executable = temp.path().join(if cfg!(windows) { "lifecycle.exe" } else { "lifecycle" });
        let mut compiler = std::process::Command::new(if cfg!(windows) { "cl" } else { "cc" });
        compiler.current_dir(temp.path());
        if cfg!(windows) {
            compiler
                .args(["/nologo", "/std:c11", "/MD"])
                .arg(format!("/I{}", include.display()))
                .arg(format!("/Fe{}", executable.display()));
        } else {
            compiler.args(["-std=c11", "-I"]).arg(include).arg("-o").arg(&executable);
        }
        let output = compiler.arg(witness).arg(bootstrap).output().unwrap();
        assert!(output.status.success(), "lifecycle witness link: {}", String::from_utf8_lossy(&output.stderr));
        let result = crate::run_linked_executable(&executable).unwrap();
        assert_eq!(result.exit_code, 0, "unit return must yield status zero: {result:?}");
        assert_eq!(result.stdout, b"shutdown-after-unit-return");
        assert!(result.stderr.is_empty());
    }

    #[test]
    fn executable_bootstrap_sources_are_embedded_not_read_from_the_build_tree() {
        let temp = tempfile::tempdir().unwrap();
        let assembly_root = super::materialize_bootstrap_sources(temp.path(), "x86_64-pc-windows-msvc").unwrap();
        let abi = Path::new(env!("CARGO_MANIFEST_DIR")).join("../beskid_abi");
        let windows = beskid_abi::generated::abi_v5_contract::ABI_V5_CORE_ARGS_ENTRY_ADAPTERS
            .iter()
            .find(|adapter| adapter.target == "x86_64-pc-windows-msvc")
            .expect("Windows Core.Args adapter");
        for source in [
            executable_bootstrap_source(&assembly_root, None),
            executable_bootstrap_source(&assembly_root, Some(windows)),
        ] {
            assert!(source.is_file(), "materialized bootstrap source missing: {}", source.display());
        }
        for (relative, contents) in super::EMBEDDED_BOOTSTRAP_SOURCES {
            let checked_in = std::fs::read_to_string(abi.join(relative)).unwrap();
            assert_eq!(*contents, checked_in, "embedded {relative} drifted from beskid_abi");
            assert!(assembly_root.parent().unwrap().join(relative).is_file());
        }
        assert!(!assembly_root.starts_with(env!("CARGO_MANIFEST_DIR")));
    }

    #[test]
    fn missing_msvc_compiler_names_the_windows_build_tools_prerequisite() {
        let error = std::process::Command::new("beskid-definitely-missing-cl").output().unwrap_err();
        let message = super::tool_unavailable("cl", error).to_string();
        assert!(message.starts_with("[E4020] Native build tool `cl` not available"));
        assert!(message.contains("Desktop development with C++"), "{message}");
    }

    #[test]
    fn missing_native_tool_is_reported_by_name_not_as_an_anonymous_linker_failure() {
        let error = std::process::Command::new("beskid-definitely-missing-assembler").output().unwrap_err();
        let message = super::tool_unavailable("beskid-definitely-missing-assembler", error).to_string();
        assert!(message.starts_with("[E4020] Native build tool `beskid-definitely-missing-assembler` not available"));
    }

    #[test]
    fn windows_platform_plan_uses_coff_sources_and_windows_toolchain_arguments() {
        let plan = platform_object_plan("x86_64-pc-windows-msvc").expect("Windows plan");

        assert_eq!(plan.assembly_source, "platform.asm");
        assert_eq!(plan.tls_source, "platform_tls.c");
        assert_eq!(plan.assembly_program, "llvm-ml");
        assert_eq!(plan.assembly_args, vec!["--m64", "/c", "/X", "/Fo"]);
        assert_eq!(plan.tls_program, "clang");
        assert_eq!(plan.tls_args, vec!["--target=x86_64-pc-windows-msvc", "-std=c11", "-c"]);
        assert_eq!(plan.object_extension, "obj");
    }

    #[test]
    fn executable_bootstrap_uses_core_args_generated_source_provenance() {
        let adapter = GeneratedCoreArgsEntryAdapter {
            target: "test-target",
            executable_entry: "main",
            program_entry: "beskid_program_main",
            capture: "utf8_argv",
            handoff: "beskid_rt_v5_args_handoff_utf8",
            ownership: "process_lifetime_copied_beskid_str_arena",
            entry_source: "generated/executable_bootstrap.c",
            os_imports: &[],
        };
        let assembly = Path::new("/runtime-assembly");

        assert_eq!(
            executable_bootstrap_source(assembly, Some(&adapter)),
            assembly.join("test-target/generated/executable_bootstrap.c")
        );
        assert_eq!(executable_bootstrap_source(assembly, None), assembly.join("common/executable_bootstrap.c"));
    }
}

/// Compile the canonical activation bootstrap against one verified test object symbol.
pub(super) fn compile_native_test_bootstrap(
    target: &str,
    core_args: Option<&GeneratedCoreArgsEntryAdapter>,
    directory: &std::path::Path,
    symbol: &str,
    control: &super::NativeExecutionControl,
    initialization: Option<(&str, u64)>,
) -> AotResult<PathBuf> {
    if !beskid_codegen::artifact::is_valid_link_name(symbol) {
        return Err(AotError::InvalidRequest { message: "invalid native test bootstrap symbol".into() });
    }
    let assembly = materialize_bootstrap_sources(directory, target)?;
    let (mut command, object) = executable_bootstrap_command(target, core_args, &assembly, directory, "test", true)?;
    command.arg(format!("-Dbeskid_program_main={symbol}"));
    configure_dynamic_initialization(&mut command, initialization)?;
    if target.contains("windows") {
        crate::windows_toolchain::configure_windows_native_command_with_control(&mut command, Some(control))?;
    }
    let output = control.run_command(&mut command, directory, "bootstrap")?;
    if !output.status.success() {
        return Err(AotError::LinkFailed {
            status: output.status.code().unwrap_or(-1),
            command: format!("{command:?}"),
            detail: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(object)
}

/// Compile compiler-issued adapter source with the existing target C tool policy and
/// the same absolute execution budget/provenance authority as native links.
pub(crate) fn compile_generated_c_object(
    target: &str,
    source: &std::path::Path,
    object: &std::path::Path,
    includes: &[PathBuf],
    control: &crate::api::NativeExecutionControl,
) -> AotResult<crate::linker::LinkToolReceipt> {
    control.check("native adapter compilation")?;
    let plan = platform_object_plan(target)?;
    let mut command = Command::new(plan.tls_program);
    command.args(&plan.tls_args);
    for include in includes {
        command.arg("-I").arg(include);
    }
    command.arg(source).arg("-o").arg(object);
    if target.contains("windows") {
        crate::windows_toolchain::configure_windows_native_command_with_control(&mut command, Some(control))?;
    }
    let mut invocation = crate::linker::LinkToolInvocation::new(command.get_program());
    invocation.args = command.get_args().map(std::ffi::OsStr::to_os_string).collect();
    invocation.environment =
        command.get_envs().map(|(key, value)| (key.to_os_string(), value.map(std::ffi::OsStr::to_os_string))).collect();
    let cwd = object
        .parent()
        .ok_or_else(|| AotError::InvalidRequest { message: "native adapter object lacks parent directory".into() })?;
    invocation.current_dir = Some(cwd.to_owned());
    let limited = control.clone().with_output_limit(1024 * 1024)?;
    let (output, receipt) = crate::linker::run_link_tool(&invocation, cwd, Some(&limited))?;
    if !output.status.success() {
        return Err(AotError::LinkFailed {
            status: output.status.code().unwrap_or(-1),
            command: format!("{} native adapter", receipt.executable.display()),
            detail: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    if !object.is_file() {
        return Err(AotError::InvalidRequest { message: "native adapter compiler produced no object".into() });
    }
    Ok(receipt)
}

fn configure_dynamic_initialization(command: &mut Command, initialization: Option<(&str, u64)>) -> AotResult<()> {
    if let Some((symbol, generation)) = initialization {
        if !beskid_codegen::artifact::is_valid_link_name(symbol) || generation == 0 {
            return Err(AotError::InvalidRequest { message: "invalid issued Dynamic initializer".into() });
        }
        command.arg(format!("-DBESKID_DYNAMIC_INITIALIZER={symbol}"));
        command.arg(format!("-DBESKID_DYNAMIC_GENERATION={generation}ULL"));
    }
    Ok(())
}
