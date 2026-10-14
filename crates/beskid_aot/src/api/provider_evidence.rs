//! Actual native provider tools, independently closed around bounded version probing.
use super::NativeExecutionControl;
use crate::{
    AotError, AotResult,
    linker::{LinkToolInvocation, LinkToolReceipt, run_link_tool},
};
use beskid_abi::runtime_kit::GlueProviderToolV1;
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

const VERSION_OUTPUT_LIMIT: usize = 1024 * 1024;
const MSVC_LINKER_BANNER: &str = "Microsoft (R) Incremental Linker Version ";

/// How a provider tool reports its version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VersionProbe {
    /// GNU-style `--version` (clang, gcc, ld, lld-link); exit status 0 is required.
    DashVersion,
    /// MSVC `link.exe` has no version option. Run without arguments it prints its startup
    /// banner ("copyright message and version number", /NOLOGO docs) on standard output and the
    /// usage text. Its exit status for that case is undocumented; the documented contract is 0 on
    /// success or the LNKnnnn error number (>= 1000) on failure, so only those are accepted and
    /// the banner line itself is the version evidence.
    MsvcLinker,
}

/// Validated version output of an actual native build tool. For `--version` probes it is the
/// tool's full stdout/stderr; for MSVC `link.exe` it is only the banner line (as stdout, with an
/// empty stderr), because the usage text that follows is not version evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ToolVersionOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Probe the exact tool named by `receipt` and fail closed unless its identity is unchanged and
/// it reports a version. `windows_target` selects the banner probe for MSVC `link.exe`.
pub(crate) fn probe_tool_version(
    receipt: &LinkToolReceipt,
    directory: &Path,
    control: &NativeExecutionControl,
    windows_target: bool,
) -> AotResult<ToolVersionOutput> {
    let probe = select_probe(&receipt.executable, windows_target);
    let mut invocation = LinkToolInvocation::new(receipt.executable.as_os_str());
    match probe {
        VersionProbe::DashVersion => invocation.args.push("--version".into()),
        VersionProbe::MsvcLinker => {
            // LINK and _LINK_ inject arguments into every link.exe run; the probe must see none.
            invocation.environment.push(("LINK".into(), None));
            invocation.environment.push(("_LINK_".into(), None));
        }
    }
    let (output, observed) = run_link_tool(&invocation, directory, Some(control))?;
    if observed.sha256 != receipt.sha256 || observed.executable != receipt.executable {
        return Err(AotError::InvalidRequest {
            message: "native provider tool changed between build and version receipt".into(),
        });
    }
    validated_version_output(probe, output.status.code(), output.stdout, output.stderr)
}

pub(super) fn version_evidence(
    role: &str,
    receipt: &LinkToolReceipt,
    directory: &Path,
    windows_linker: bool,
) -> AotResult<GlueProviderToolV1> {
    let control = NativeExecutionControl::new(Instant::now() + Duration::from_secs(10), Arc::new(|| false));
    let output = probe_tool_version(receipt, directory, &control, windows_linker)?;
    Ok(GlueProviderToolV1 {
        role: role.into(),
        executable_sha256: receipt.sha256.clone(),
        version_output_sha256: provider_version_digest(&output),
    })
}

fn provider_version_digest(output: &ToolVersionOutput) -> String {
    let mut digest = Sha256::new();
    digest.update((output.stdout.len() as u64).to_le_bytes());
    digest.update(&output.stdout);
    digest.update((output.stderr.len() as u64).to_le_bytes());
    digest.update(&output.stderr);
    format!("{:x}", digest.finalize())
}

fn select_probe(executable: &Path, windows_target: bool) -> VersionProbe {
    let is_msvc_link = executable
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem.eq_ignore_ascii_case("link"));
    if windows_target && is_msvc_link { VersionProbe::MsvcLinker } else { VersionProbe::DashVersion }
}

fn probe_failed(detail: &str) -> AotError {
    AotError::InvalidRequest {
        message: format!("native provider version probe failed or exceeded bounded output: {detail}"),
    }
}

fn validated_version_output(
    probe: VersionProbe,
    status: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
) -> AotResult<ToolVersionOutput> {
    if stdout.len().saturating_add(stderr.len()) > VERSION_OUTPUT_LIMIT {
        return Err(probe_failed("output exceeds 1 MiB"));
    }
    match probe {
        VersionProbe::DashVersion => {
            if status != Some(0) {
                return Err(probe_failed(&format!("--version exited with {status:?}")));
            }
            if stdout.is_empty() && stderr.is_empty() {
                return Err(probe_failed("--version produced no output"));
            }
            Ok(ToolVersionOutput { stdout, stderr })
        }
        VersionProbe::MsvcLinker => {
            match status {
                Some(0) => {}
                Some(code) if code >= 1000 => {}
                other => return Err(probe_failed(&format!("link.exe exited with {other:?}"))),
            }
            let banner = msvc_linker_banner(&stdout).ok_or_else(|| probe_failed("link.exe printed no version banner"))?;
            Ok(ToolVersionOutput { stdout: format!("{banner}\n").into_bytes(), stderr: Vec::new() })
        }
    }
}

/// Returns the exact `Microsoft (R) Incremental Linker Version X.Y.Z.W` line, or `None` when the
/// version is missing or malformed.
fn msvc_linker_banner(stdout: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(stdout).ok()?;
    text.lines().map(str::trim).find_map(|line| {
        let version = line.strip_prefix(MSVC_LINKER_BANNER)?;
        let parts: Vec<&str> = version.split('.').collect();
        (parts.len() >= 2 && parts.iter().all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())))
            .then(|| line.to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BANNER: &[u8] = b"Microsoft (R) Incremental Linker Version 14.44.35215.0\r\n\
Copyright (C) Microsoft Corporation.  All rights reserved.\r\n\r\n usage: LINK [options] [files] [@commandfile]\r\n";

    #[test]
    fn selects_banner_probe_only_for_windows_msvc_link() {
        assert_eq!(select_probe(Path::new("C:/VS/bin/Hostx64/x64/link.exe"), true), VersionProbe::MsvcLinker);
        assert_eq!(select_probe(Path::new("C:/VS/bin/Hostx64/x64/LINK.EXE"), true), VersionProbe::MsvcLinker);
        assert_eq!(select_probe(Path::new("C:/LLVM/bin/lld-link.exe"), true), VersionProbe::DashVersion);
        assert_eq!(select_probe(Path::new("/usr/bin/link"), false), VersionProbe::DashVersion);
    }

    fn validate(probe: VersionProbe, status: Option<i32>, stdout: &[u8], stderr: &[u8]) -> AotResult<ToolVersionOutput> {
        validated_version_output(probe, status, stdout.to_vec(), stderr.to_vec())
    }

    #[test]
    fn msvc_banner_is_the_version_evidence_for_documented_exit_codes() {
        // link.exe 14.44 exits 1100 when run without arguments (observed on the Windows agent).
        let expected = ToolVersionOutput {
            stdout: b"Microsoft (R) Incremental Linker Version 14.44.35215.0\n".to_vec(),
            stderr: Vec::new(),
        };
        for status in [Some(0), Some(1100), Some(1561)] {
            assert_eq!(validate(VersionProbe::MsvcLinker, status, BANNER, b"").unwrap(), expected);
        }
    }

    #[test]
    fn msvc_probe_fails_closed() {
        for status in [None, Some(1), Some(128), Some(-1)] {
            assert!(validate(VersionProbe::MsvcLinker, status, BANNER, b"").is_err());
        }
        for stdout in [
            &b""[..],
            b"usage: LINK [options]\r\n",
            b"Microsoft (R) Incremental Linker Version \r\n",
            b"Microsoft (R) Incremental Linker Version 14\r\n",
            b"Microsoft (R) Incremental Linker Version 14.x.1\r\n",
        ] {
            assert!(validate(VersionProbe::MsvcLinker, Some(0), stdout, b"").is_err());
        }
        let oversized = [BANNER, &vec![b' '; VERSION_OUTPUT_LIMIT]].concat();
        assert!(validate(VersionProbe::MsvcLinker, Some(0), &oversized, b"").is_err());
    }

    #[test]
    fn dash_version_requires_success_and_output() {
        assert!(validate(VersionProbe::DashVersion, Some(0), b"clang version 18.1.8\n", b"").is_ok());
        assert!(validate(VersionProbe::DashVersion, Some(1), b"clang version 18.1.8\n", b"").is_err());
        assert!(validate(VersionProbe::DashVersion, Some(0), b"", b"").is_err());
    }
}
