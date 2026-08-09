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

pub(super) fn compile_core_args_entry_adapter(
    adapter: &GeneratedCoreArgsEntryAdapter,
    output_dir: &std::path::Path,
    name: &str,
) -> AotResult<PathBuf> {
    let plan = platform_object_plan(adapter.target)?;
    let assembly_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../beskid_abi/assembly").join(adapter.target);
    let source = assembly_root.join(adapter.entry_source);
    let object = output_dir.join(format!("{name}.core_args_entry.{}", plan.object_extension));
    let mut command = Command::new(plan.assembly_program);
    command.args(&plan.assembly_args);
    if plan.assembly_output_before_source {
        command.arg(&object).arg(&source);
    } else {
        command.arg(&source).arg("-o").arg(&object);
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
    use super::platform_object_plan;

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
}
