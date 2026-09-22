use std::path::PathBuf;
use std::process::Command;

use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata, render_runtime_asm_include};
use beskid_abi::generated::abi_v5_contract::GeneratedCoreArgsEntryAdapter;
use cargo_cross::config::{Arch, Os, get_target_config};

use crate::error::{AotError, AotResult};

pub(super) fn compile_context_assembly(
    target: &TargetMetadata,
    output_dir: &std::path::Path,
    name: &str,
) -> AotResult<PathBuf> {
    let assembly_root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../beskid_abi/assembly").join(target.triple.as_str());
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
    let output = command.output().map_err(|_| AotError::LinkerUnavailable)?;
    if !output.status.success() {
        return Err(AotError::LinkFailed {
            status: output.status.code().unwrap_or(-1),
            command: format!("{:?}", command),
            detail: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(object)
}
pub(super) fn compile_platform_objects(
    target: &TargetMetadata,
    output_dir: &std::path::Path,
    name: &str,
) -> AotResult<Vec<PathBuf>> {
    let plan = platform_object_plan(target.triple.as_str())?;
    let assembly_root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../beskid_abi/assembly").join(target.triple.as_str());
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
    let output = assembly.output().map_err(|_| AotError::LinkerUnavailable)?;
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
    let output = Command::new(plan.tls_program)
        .args(&plan.tls_args)
        .arg(&tls_source)
        .arg("-o")
        .arg(&tls_object)
        .output()
        .map_err(|_| AotError::LinkerUnavailable)?;
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
    let output = Command::new(plan.tls_program)
        .args(&plan.tls_args)
        .arg(&adapter_source)
        .arg("-o")
        .arg(&adapter_object)
        .output()
        .map_err(|_| AotError::LinkerUnavailable)?;
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
    Ok(vec![object, tls_object, adapter_object])
}

pub(super) fn compile_executable_bootstrap(
    target: &str,
    core_args: Option<&GeneratedCoreArgsEntryAdapter>,
    output_dir: &std::path::Path,
    name: &str,
    program_returns_void: bool,
) -> AotResult<PathBuf> {
    let (mut command, object) =
        executable_bootstrap_command(target, core_args, output_dir, name, program_returns_void)?;
    let output = command.output().map_err(|_| AotError::LinkerUnavailable)?;
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
    output_dir: &std::path::Path,
    name: &str,
    program_returns_void: bool,
) -> AotResult<(Command, PathBuf)> {
    let plan = platform_object_plan(target)?;
    let assembly_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../beskid_abi/assembly");
    let source = executable_bootstrap_source(&assembly_root, core_args);
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
        command.arg(format!("/Fo{}", object.display())).arg(&source);
    } else {
        command.arg(&source).arg("-o").arg(&object);
    }
    Ok((command, object))
}

/// Resolve the bootstrap source from the generated Core.Args provenance when it is active.
/// Non-argument executables use the same common source directly.
fn executable_bootstrap_source(
    assembly_root: &std::path::Path,
    core_args: Option<&GeneratedCoreArgsEntryAdapter>,
) -> PathBuf {
    core_args
        .map(|adapter| assembly_root.join(&adapter.target).join(&adapter.entry_source))
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
        let (command, _) =
            executable_bootstrap_command("x86_64-pc-windows-msvc", None, Path::new("build"), "app", false)
                .expect("Windows bootstrap command");
        assert_eq!(command.get_program(), "cl");
        let args = command.get_args().collect::<Vec<_>>();
        assert!(args.iter().any(|arg| *arg == "/MD"), "bootstrap must select dynamic CRT defaults: {command:?}");
        assert!(!args.iter().any(|arg| *arg == "/MT" || *arg == "/MDd"));
    }

    #[test]
    fn unit_executable_host_initializes_zeroed_state_and_shuts_down_after_program_return() {
        let target = crate::target::detect_target(None).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let bootstrap = super::compile_executable_bootstrap(&target.triple, None, temp.path(), "unit", true).unwrap();
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
