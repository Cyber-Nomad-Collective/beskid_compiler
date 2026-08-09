use std::process::Command;

use crate::error::{AotError, AotResult};

use super::LinkRequest;

pub(super) fn append_library_search_paths(req: &LinkRequest, target: &str, cmd: &mut Command) -> AotResult<()> {
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

pub(super) fn append_external_libraries(req: &LinkRequest, target: &str, cmd: &mut Command) -> AotResult<()> {
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

pub(super) fn append_export_policy_flags(req: &LinkRequest, target: &str, cmd: &mut Command) -> AotResult<()> {
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
        std::fs::write(&script_path, script)
            .map_err(|err| AotError::Io { path: script_path.clone(), message: err.to_string() })?;
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
