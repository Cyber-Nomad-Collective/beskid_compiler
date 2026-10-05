use cargo_cross::config::HostPlatform;

use std::path::Path;
use std::process::{Command, Output};

/// Normalize a logical library identity to its native short name without changing
/// its spelling. Explicit `-l` flags already encode a native name and pass through.
pub fn canonical_link_library_name(logical: &str) -> String {
    let logical = logical.trim();
    if logical.starts_with("-l") {
        return logical.to_owned();
    }
    let name = logical.strip_prefix("lib").unwrap_or(logical);
    // A versioned ELF soname (`libc.so.6`, `libssl.so.3.0`) links as its base name: `-lc`.
    if let Some((base, version)) = name.split_once(".so.")
        && !version.is_empty()
        && version.split('.').all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return base.to_owned();
    }
    name.strip_suffix(".so")
        .or_else(|| name.strip_suffix(".dylib"))
        .or_else(|| name.strip_suffix(".a"))
        .unwrap_or(name)
        .to_owned()
}

#[cfg(test)]
mod canonical_link_library_name_tests {
    use super::canonical_link_library_name;

    #[test]
    fn versioned_sonames_link_by_base_name() {
        assert_eq!(canonical_link_library_name("libc.so.6"), "c");
        assert_eq!(canonical_link_library_name("libssl.so.3.0"), "ssl");
        assert_eq!(canonical_link_library_name("libm.so"), "m");
        assert_eq!(canonical_link_library_name("libc"), "c");
        assert_eq!(canonical_link_library_name("libfoo.so.x"), "foo.so.x");
        assert_eq!(canonical_link_library_name("-lpthread"), "-lpthread");
    }
}

pub(super) fn detect_c_compiler() -> String {
    if let Ok(value) = std::env::var("CC") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return trimmed.to_owned();
        }
    }

    let host = HostPlatform::detect();
    if host.is_windows() { "cl".to_owned() } else { "cc".to_owned() }
}

pub(super) fn append_static_archive(cmd: &mut Command, target: &str, archive: &Path) {
    if target.contains("darwin") || target.contains("macos") {
        cmd.arg("-Wl,-force_load").arg(archive);
    } else {
        cmd.arg(archive);
    }
}

pub(super) fn format_link_command(command: &Command) -> String {
    format!("{command:?}")
}

pub(super) fn format_link_detail(output: &Output) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_command_diagnostics_preserve_actual_library_arguments() {
        let mut command = Command::new("cc");
        command.args(["runtime.o", "-L/sdk/lib", "-lSystem", "-shared"]);
        let rendered = format_link_command(&command);
        assert!(rendered.contains("-L/sdk/lib"), "{rendered}");
        assert!(rendered.contains("-lSystem"), "{rendered}");
    }
}
