#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use beskid_abi::BESKID_USER_FFI_LAYOUT_BAND;
use beskid_aot::{AotBuildRequest, BuildOutputKind, build};
use beskid_codegen::services::lower_source;
use beskid_runtime::{CallbackTableEntry, beskid_register_callbacks};

fn temp_case_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time ok")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "beskid_engine_ffi_v03_{name}_{}_{}",
        std::process::id(),
        nanos
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn link_time_extern_getpid_matches_platform_spec() -> Result<()> {
    let src = r#"
[Extern(Abi:"C", Library:"libc")]
pub contract Libc {
    i64 getpid();
}

pub i64 main() { return Libc.getpid(); }
"#;
    let lowered = lower_source(Path::new("<memory>"), src, false)?;
    let dir = temp_case_dir("getpid");
    let output = dir.join("getpid_test");
    let result = build(AotBuildRequest {
        external_libraries: vec!["c".into()],
        ..AotBuildRequest::with_defaults(lowered.artifact, BuildOutputKind::Exe, output, "main")
    })?;
    let binary = result.final_path.expect("linked executable path");
    let mut child = Command::new(&binary).spawn()?;
    let expected = (child.id() & 0xFF) as i32;
    let status = child.wait()?;
    assert_eq!(status.code(), Some(expected));
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}

#[test]
fn export_plugin_init_visible_to_linker() -> Result<()> {
    let src = r#"
[Export(Abi:"C", Symbol:"beskid_plugin_init")]
pub unit plugin_init() { return; }
"#;
    let lowered = lower_source(Path::new("<memory>"), src, false)?;
    assert!(
        lowered
            .artifact
            .exports
            .iter()
            .any(|e| e.exported_symbol == "beskid_plugin_init")
    );
    let dir = temp_case_dir("export_so");
    let output = dir.join("libplugin.so");
    let result = build(AotBuildRequest::with_defaults(
        lowered.artifact,
        BuildOutputKind::SharedLib,
        output.clone(),
        "plugin_init",
    ))?;
    let shared = result.final_path.expect("shared library path");
    let nm = Command::new("nm").arg("-D").arg(&shared).output()?;
    assert!(
        nm.status.success(),
        "nm failed: {}",
        String::from_utf8_lossy(&nm.stderr)
    );
    let stdout = String::from_utf8_lossy(&nm.stdout);
    assert!(
        stdout
            .lines()
            .any(|line| line.contains("beskid_plugin_init")),
        "expected exported symbol in nm output:\n{stdout}"
    );
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}

#[test]
fn host_registers_callbacks_with_layout_band() -> Result<()> {
    let table = [CallbackTableEntry {
        symbol_id: 1,
        fn_ptr: std::ptr::null(),
        userdata: std::ptr::null_mut(),
    }];
    assert_eq!(
        unsafe {
            beskid_register_callbacks(BESKID_USER_FFI_LAYOUT_BAND, table.as_ptr(), table.len())
        },
        0
    );
    assert_eq!(
        unsafe {
            beskid_register_callbacks(BESKID_USER_FFI_LAYOUT_BAND - 1, table.as_ptr(), table.len())
        },
        1
    );
    Ok(())
}
