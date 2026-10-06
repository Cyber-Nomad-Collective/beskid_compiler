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

pub(super) fn version_evidence(
    role: &str,
    receipt: &LinkToolReceipt,
    directory: &Path,
    windows_linker: bool,
) -> AotResult<GlueProviderToolV1> {
    let mut invocation = LinkToolInvocation::new(receipt.executable.as_os_str());
    invocation.args.push(if windows_linker { "/help" } else { "--version" }.into());
    let control = NativeExecutionControl::new(Instant::now() + Duration::from_secs(10), Arc::new(|| false));
    let (output, observed) = run_link_tool(&invocation, directory, Some(&control))?;
    if observed.sha256 != receipt.sha256 || observed.executable != receipt.executable {
        return Err(AotError::InvalidRequest {
            message: "native provider tool changed between build and version receipt".into(),
        });
    }
    if !output.status.success()
        || output.stdout.len().saturating_add(output.stderr.len()) > 1024 * 1024
        || (output.stdout.is_empty() && output.stderr.is_empty())
    {
        return Err(AotError::InvalidRequest {
            message: "native provider version probe failed or exceeded bounded output".into(),
        });
    }
    let mut digest = Sha256::new();
    digest.update((output.stdout.len() as u64).to_le_bytes());
    digest.update(&output.stdout);
    digest.update((output.stderr.len() as u64).to_le_bytes());
    digest.update(&output.stderr);
    Ok(GlueProviderToolV1 {
        role: role.into(),
        executable_sha256: observed.sha256,
        version_output_sha256: format!("{:x}", digest.finalize()),
    })
}
