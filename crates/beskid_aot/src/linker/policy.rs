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

/// The logical `[Extern(Library: "beskid_runtime")]` library is the canonical runtime itself.
/// When the request links a canonical runtime input (static archive or shared provider), that
/// input already supplies every runtime export; naming it again as `-lbeskid_runtime` would ask
/// the linker to search for a library that the validated runtime path replaces.
fn supplied_by_runtime_input(req: &LinkRequest, library: &str) -> bool {
    req.runtime.is_some() && canonical_link_library_name(library) == "beskid_runtime"
}

pub(super) fn append_external_libraries(req: &LinkRequest, target: &str, cmd: &mut Command) -> AotResult<()> {
    if req.external_libraries.is_empty() {
        return Ok(());
    }
    if target.contains("windows") {
        for library in req.external_libraries.iter().filter(|library| !supplied_by_runtime_input(req, library)) {
            cmd.arg(format!("{}.lib", library.trim()));
        }
        return Ok(());
    }
    for library in &req.external_libraries {
        let name = canonical_link_library_name(library);
        if name.is_empty() || supplied_by_runtime_input(req, library) {
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
/// ELF has no weak `DT_NEEDED`: a library found at link time becomes a load-time dependency (also
/// under a default `--as-needed`), and one not found is not referenced at all. Mach-O links a found library with `-weak-l`, so the
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
                // Generated code references the library only through weak symbols, and GNU ld
                // `--as-needed` (the default of Debian and Ubuntu `gcc`) drops a library that
                // satisfies only weak references. Force the `DT_NEEDED` entry for this library.
                cmd.arg("-Wl,--push-state,--no-as-needed");
                cmd.arg(format!("-l{name}"));
                cmd.arg("-Wl,--pop-state");
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

#[cfg(test)]
mod optional_library_tests {
    use std::path::PathBuf;
    use std::process::Command;

    use super::append_optional_libraries;
    use crate::api::{BuildOutputKind, LinkMode};
    use crate::linker::{LinkRequest, OptionalLinkLibrary};

    #[test]
    fn a_found_elf_optional_library_is_needed_also_under_as_needed() {
        let search = tempfile::tempdir().expect("temp dir");
        std::fs::write(search.path().join("libcrypto.so"), b"").expect("library stub");
        let req = LinkRequest {
            target_triple: Some("x86_64-unknown-linux-gnu".into()),
            output_kind: BuildOutputKind::Exe,
            output_path: PathBuf::from("out/app"),
            object_path: PathBuf::from("app.o"),
            additional_object_paths: Vec::new(),
            runtime: None,
            host_staticlib: None,
            entrypoint_symbol: String::new(),
            exported_symbols: Vec::new(),
            link_mode: LinkMode::PreferDynamic,
            verbose: false,
            external_libraries: Vec::new(),
            optional_libraries: vec![OptionalLinkLibrary {
                library: "libcrypto.so.3".into(),
                symbols: vec!["EVP_CIPHER_CTX_new".into()],
            }],
            library_search_paths: vec![search.path().to_path_buf()],
        };
        let mut command = Command::new("cc");
        append_optional_libraries(&req, "x86_64-unknown-linux-gnu", "cc", &mut command).expect("optional libraries");
        let args = command.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect::<Vec<_>>();
        assert_eq!(args, ["-Wl,--push-state,--no-as-needed", "-lcrypto", "-Wl,--pop-state"]);
    }
}
