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

pub(super) fn version_evidence(
    role: &str,
    receipt: &LinkToolReceipt,
    directory: &Path,
    windows_linker: bool,
) -> AotResult<GlueProviderToolV1> {
    let probe = select_probe(&receipt.executable, windows_linker);
    let mut invocation = LinkToolInvocation::new(receipt.executable.as_os_str());
    match probe {
        VersionProbe::DashVersion => invocation.args.push("--version".into()),
        VersionProbe::MsvcLinker => {
            // LINK and _LINK_ inject arguments into every link.exe run; the probe must see none.
            invocation.environment.push(("LINK".into(), None));
            invocation.environment.push(("_LINK_".into(), None));
        }
    }
    let control = NativeExecutionControl::new(Instant::now() + Duration::from_secs(10), Arc::new(|| false));
    let (output, observed) = run_link_tool(&invocation, directory, Some(&control))?;
    if observed.sha256 != receipt.sha256 || observed.executable != receipt.executable {
        return Err(AotError::InvalidRequest {
            message: "native provider tool changed between build and version receipt".into(),
        });
    }
    let version_output_sha256 = version_output_digest(probe, output.status.code(), &output.stdout, &output.stderr)?;
    Ok(GlueProviderToolV1 { role: role.into(), executable_sha256: observed.sha256, version_output_sha256 })
}

fn select_probe(executable: &Path, windows_linker: bool) -> VersionProbe {
    let is_msvc_link = executable
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem.eq_ignore_ascii_case("link"));
    if windows_linker && is_msvc_link { VersionProbe::MsvcLinker } else { VersionProbe::DashVersion }
}

fn probe_failed(detail: &str) -> AotError {
    AotError::InvalidRequest {
        message: format!("native provider version probe failed or exceeded bounded output: {detail}"),
    }
}

fn version_output_digest(probe: VersionProbe, status: Option<i32>, stdout: &[u8], stderr: &[u8]) -> AotResult<String> {
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
            let mut digest = Sha256::new();
            digest.update((stdout.len() as u64).to_le_bytes());
            digest.update(stdout);
            digest.update((stderr.len() as u64).to_le_bytes());
            digest.update(stderr);
            Ok(format!("{:x}", digest.finalize()))
        }
        VersionProbe::MsvcLinker => {
            match status {
                Some(0) => {}
                Some(code) if code >= 1000 => {}
                other => return Err(probe_failed(&format!("link.exe exited with {other:?}"))),
            }
            let banner = msvc_linker_banner(stdout).ok_or_else(|| probe_failed("link.exe printed no version banner"))?;
            Ok(format!("{:x}", Sha256::digest(banner.as_bytes())))
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

    #[test]
    fn msvc_banner_is_the_version_evidence_for_documented_exit_codes() {
        let expected = format!("{:x}", Sha256::digest(b"Microsoft (R) Incremental Linker Version 14.44.35215.0"));
        for status in [Some(0), Some(1561), Some(4044)] {
            assert_eq!(version_output_digest(VersionProbe::MsvcLinker, status, BANNER, b"").unwrap(), expected);
        }
    }

    #[test]
    fn msvc_probe_fails_closed() {
        for status in [None, Some(1), Some(128), Some(-1)] {
            assert!(version_output_digest(VersionProbe::MsvcLinker, status, BANNER, b"").is_err());
        }
        for stdout in [
            &b""[..],
            b"usage: LINK [options]\r\n",
            b"Microsoft (R) Incremental Linker Version \r\n",
            b"Microsoft (R) Incremental Linker Version 14\r\n",
            b"Microsoft (R) Incremental Linker Version 14.x.1\r\n",
        ] {
            assert!(version_output_digest(VersionProbe::MsvcLinker, Some(0), stdout, b"").is_err());
        }
        let oversized = [BANNER, &vec![b' '; VERSION_OUTPUT_LIMIT]].concat();
        assert!(version_output_digest(VersionProbe::MsvcLinker, Some(0), &oversized, b"").is_err());
    }

    #[test]
    fn dash_version_requires_success_and_output() {
        assert!(version_output_digest(VersionProbe::DashVersion, Some(0), b"clang version 18.1.8\n", b"").is_ok());
        assert!(version_output_digest(VersionProbe::DashVersion, Some(1), b"clang version 18.1.8\n", b"").is_err());
        assert!(version_output_digest(VersionProbe::DashVersion, Some(0), b"", b"").is_err());
    }
}
