//! Target triple normalization and filename extensions for objects and linked artifacts.

use crate::api::BuildOutputKind;
use crate::error::{AotError, AotResult};
use cargo_cross::config::{HostPlatform, Os, get_target_config};

/// Resolved triple string plus platform-specific file extensions.
#[derive(Debug, Clone)]
pub struct TargetInfo {
    pub triple: String,
    pub object_ext: &'static str,
    pub static_lib_ext: &'static str,
    pub shared_lib_ext: &'static str,
    pub exe_ext: &'static str,
}

/// Infer [`TargetInfo`] from `triple_override` or host platform; uses [`cargo_cross::config`]
/// for structured target knowledge, then maps to platform extensions.
pub fn detect_target(triple_override: Option<&str>) -> AotResult<TargetInfo> {
    let triple = if let Some(explicit) = triple_override {
        explicit.to_owned()
    } else {
        let host = HostPlatform::detect();
        host.triple
    };

    let config = get_target_config(&triple).ok_or_else(|| AotError::UnsupportedOutputKind {
        target: triple.clone(),
        kind: BuildOutputKind::ObjectOnly,
    })?;

    match config.os {
        Os::Windows => Ok(TargetInfo {
            triple,
            object_ext: "obj",
            static_lib_ext: "lib",
            shared_lib_ext: "dll",
            exe_ext: "exe",
        }),
        Os::Darwin | Os::Ios | Os::IosSim => Ok(TargetInfo {
            triple,
            object_ext: "o",
            static_lib_ext: "a",
            shared_lib_ext: "dylib",
            exe_ext: "",
        }),
        Os::Linux | Os::FreeBsd | Os::Android => Ok(TargetInfo {
            triple,
            object_ext: "o",
            static_lib_ext: "a",
            shared_lib_ext: "so",
            exe_ext: "",
        }),
        _ => Err(AotError::UnsupportedOutputKind {
            target: triple,
            kind: BuildOutputKind::ObjectOnly,
        }),
    }
}

/// Default filename for `kind` on `target` (e.g. `libfoo.so`, `hello.exe`, `hello.o`).
pub fn output_filename(base: &str, kind: BuildOutputKind, target: &TargetInfo) -> String {
    match kind {
        BuildOutputKind::ObjectOnly => format!("{base}.{}", target.object_ext),
        BuildOutputKind::Exe => {
            if target.exe_ext.is_empty() {
                base.to_string()
            } else {
                format!("{base}.{}", target.exe_ext)
            }
        }
        BuildOutputKind::StaticLib => {
            if target.static_lib_ext == "lib" {
                format!("{base}.lib")
            } else {
                format!("lib{base}.{}", target.static_lib_ext)
            }
        }
        BuildOutputKind::SharedLib => {
            if target.shared_lib_ext == "dll" {
                format!("{base}.dll")
            } else {
                format!("lib{base}.{}", target.shared_lib_ext)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_linux_object_name() {
        let target = TargetInfo {
            triple: "x86_64-unknown-linux-gnu".to_string(),
            object_ext: "o",
            static_lib_ext: "a",
            shared_lib_ext: "so",
            exe_ext: "",
        };

        assert_eq!(
            output_filename("hello", BuildOutputKind::ObjectOnly, &target),
            "hello.o"
        );
    }

    #[test]
    fn computes_windows_static_name() {
        let target = TargetInfo {
            triple: "x86_64-pc-windows-msvc".to_string(),
            object_ext: "obj",
            static_lib_ext: "lib",
            shared_lib_ext: "dll",
            exe_ext: "exe",
        };

        assert_eq!(
            output_filename("hello", BuildOutputKind::StaticLib, &target),
            "hello.lib"
        );
    }
}
