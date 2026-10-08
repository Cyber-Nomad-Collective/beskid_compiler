use std::process::Command;

use crate::error::{AotError, AotResult};

use super::{LinkRequest, canonical_link_library_name};

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
        let name = canonical_link_library_name(library);
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

/// Link each optional `[Extern]` library the linker can find; leave the rest unlinked so their
/// weak undefined symbols resolve to null.
///
/// ELF has no weak `DT_NEEDED`: a library found at link time becomes a load-time dependency, and
/// one not found is not referenced at all. Mach-O links a found library with `-weak-l`, so the
/// executable also loads where it is absent, and marks the symbols of a missing library as
/// allowed to stay undefined. COFF weak externals are not supported, so a Windows target with an
/// optional contract fails closed.
pub(super) fn append_optional_libraries(
    req: &LinkRequest,
    target: &str,
    compiler: &str,
    cmd: &mut Command,
) -> AotResult<()> {
    if req.optional_libraries.is_empty() {
        return Ok(());
    }
    if target.contains("windows") {
        let libraries = req.optional_libraries.iter().map(|entry| entry.library.as_str()).collect::<Vec<_>>();
        return Err(AotError::InvalidRequest {
            message: format!(
                "optional [Extern] contracts are not supported for Windows targets (libraries: {})",
                libraries.join(", ")
            ),
        });
    }
    let macho = target.contains("darwin") || target.contains("apple") || target.contains("macos");
    if macho {
        // The absent-symbol sentinel generated code compares against is never defined in AOT.
        cmd.arg(format!("-Wl,-U,_{}", beskid_codegen::OPTIONAL_EXTERN_ABSENT_SYMBOL));
    }
    for entry in &req.optional_libraries {
        let name = canonical_link_library_name(&entry.library);
        let name = name.strip_prefix("-l").unwrap_or(&name).to_owned();
        if name.is_empty() {
            continue;
        }
        let found = optional_library_available(req, compiler, &name, macho);
        match (macho, found) {
            (true, true) => {
                cmd.arg(format!("-Wl,-weak-l{name}"));
            }
            (true, false) => {
                for symbol in &entry.symbols {
                    cmd.arg(format!("-Wl,-U,_{}", symbol.strip_prefix('_').unwrap_or(symbol)));
                }
            }
            (false, true) => {
                cmd.arg(format!("-l{name}"));
            }
            (false, false) => {}
        }
    }
    Ok(())
}

/// Whether the linker can resolve `-l<name>`: a candidate file in an explicit search path, or one
/// the C compiler driver reports through `-print-file-name`.
fn optional_library_available(req: &LinkRequest, compiler: &str, name: &str, macho: bool) -> bool {
    let candidates = if macho {
        vec![format!("lib{name}.dylib"), format!("lib{name}.tbd"), format!("lib{name}.a")]
    } else {
        vec![format!("lib{name}.so"), format!("lib{name}.a")]
    };
    if req.library_search_paths.iter().any(|path| candidates.iter().any(|candidate| path.join(candidate).is_file())) {
        return true;
    }
    candidates.iter().any(|candidate| {
        Command::new(compiler)
            .arg(format!("-print-file-name={candidate}"))
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
            .is_some_and(|reported| {
                let path = std::path::Path::new(&reported);
                path.is_absolute() && path.is_file()
            })
    })
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
        // MSVC link.exe/lld-link accept `/EXPORT:` flags, but a module-definition (`.def`) file
        // is the canonical, command-line-length-independent export contract for a DLL. It mirrors
        // the Linux version-script path and reliably exports every ABI-v5 lifecycle symbol
        // (assembly context switches + Rust `#[no_mangle]` runtime entry points) from the
        // linked image, which per-symbol `/EXPORT:` flags fail to surface on Windows hosts.
        let def_path = req.output_path.with_extension("exports.def");
        let mut script = String::from("EXPORTS\n");
        for symbol in &req.exported_symbols {
            script.push_str(&format!("    {symbol}\n"));
        }
        std::fs::write(&def_path, script)
            .map_err(|err| AotError::Io { path: def_path.clone(), message: err.to_string() })?;
        cmd.arg(format!("/DEF:{}", def_path.display()));
        return Ok(());
    }

    Err(AotError::UnsupportedLinkerStrategy {
        target: target.to_owned(),
        message: "shared export policy flags are not implemented for this target".to_owned(),
    })
}
