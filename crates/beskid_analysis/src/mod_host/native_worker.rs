//! Worker-side CABI2 execution against retained, exact-image host leases.
//! This module never receives the parent's borrowed syntax/semantic authority.
use super::native_wire::{NATIVE_CALLBACKS_V2, NativeEntryV2, NativeHeaderV2, WireArena, WireTypes, WorkerValues};
use anyhow::{Context, Result, bail};
use beskid_artifacts::native_host::NativeHostLease;
use std::ffi::CString;

pub(crate) struct WorkerEntry<'a> {
    pub symbol: &'a str,
    pub discriminator: &'a str,
    pub request_type: u32,
    pub factory_request_type: u32,
    pub result_type: u32,
}

/// Invoke only inside the isolated worker. The caller supplies the retained host
/// lease and the source-to-native qualification's exact entry/type metadata.
pub(crate) unsafe fn invoke_entry(
    lease: &NativeHostLease,
    image: usize,
    entry: WorkerEntry<'_>,
    types: &WireTypes,
    generation: u64,
    invocation: u64,
    request: &serde_json::Value,
    factory_request: &serde_json::Value,
    service: &mut dyn FnMut(&str, &[u64], &mut WireArena) -> Result<u64>,
) -> Result<serde_json::Value> {
    if generation == 0 || invocation == 0 {
        bail!("native invocation requires nonzero issued generation and session");
    }
    let discriminator = CString::new(entry.discriminator).context("invalid native discriminator symbol")?;
    let discriminator = unsafe { lease.images().image_symbol::<unsafe extern "C" fn() -> u32>(image, &discriminator) }?;
    if unsafe { discriminator() } != 2 {
        bail!("native Mod executable does not implement CABI2");
    }
    let symbol = CString::new(entry.symbol).context("invalid native contract symbol")?;
    let callable = unsafe { lease.images().image_symbol::<NativeEntryV2>(image, &symbol) }?;
    let mut arena = WireArena::new();
    let request = types.build(&mut arena, entry.request_type, request)?;
    let factory_request = types.build(&mut arena, entry.factory_request_type, factory_request)?;
    let mut values = WorkerValues { arena, record_types: types.record_types(), service, error: None };
    let header = NativeHeaderV2 {
        version: 2,
        bytes: u32::try_from(std::mem::size_of::<NativeHeaderV2>())?,
        generation,
        invocation,
        context: std::ptr::from_mut(&mut values).cast(),
        callbacks: &NATIVE_CALLBACKS_V2,
        request,
        factory_request,
    };
    let mut result = 0;
    let status = unsafe { callable(&header, &mut result) };
    if let Some(error) = values.error {
        bail!("native contract callback failed: {error}");
    }
    if status != 0 || result == 0 {
        bail!("native contract failed with status {status} and result {result}");
    }
    types.json(&values.arena, result, entry.result_type)
}

/// Resolve entry geometry from the immutable, inventory-verified producer plan.
/// Entry symbols are never guessed from registration names.
pub(crate) fn entry_from_plan<'a>(plan: &'a serde_json::Value, symbol: &'a str) -> Result<WorkerEntry<'a>> {
    let entries = plan.get("entries").and_then(serde_json::Value::as_array).context("native adapter entries absent")?;
    let mut matching =
        entries.iter().filter(|entry| entry.get("symbol").and_then(serde_json::Value::as_str) == Some(symbol));
    let entry = matching.next().context("native requested entry is not producer-issued")?;
    if matching.next().is_some() {
        bail!("native producer entry identity is ambiguous");
    }
    let id = |name: &str| -> Result<u32> {
        u32::try_from(
            entry
                .get(name)
                .and_then(serde_json::Value::as_u64)
                .with_context(|| format!("native entry {name} absent"))?,
        )
        .context("native entry type ID overflow")
    };
    Ok(WorkerEntry {
        symbol,
        discriminator: plan
            .get("discriminator_symbol")
            .and_then(serde_json::Value::as_str)
            .context("native discriminator absent")?,
        request_type: id("request_type")?,
        factory_request_type: id("factory_request_type")?,
        result_type: id("result_type")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_worker_rejects_unissued_and_duplicate_entries() {
        let entry = serde_json::json!({"symbol":"issued","request_type":0,"factory_request_type":1,"result_type":2});
        let plan = serde_json::json!({"discriminator_symbol":"version","entries":[entry.clone()]});
        assert!(entry_from_plan(&plan, "guessed").is_err());
        let geometry = entry_from_plan(&plan, "issued").unwrap();
        assert_eq!(geometry.factory_request_type, 1);
        let ambiguous = serde_json::json!({"discriminator_symbol":"version","entries":[entry.clone(),entry]});
        assert!(entry_from_plan(&ambiguous, "issued").is_err());
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkerEndpoint {
    address: std::net::SocketAddr,
    token: String,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkerInvocation {
    descriptor: std::path::PathBuf,
    descriptor_sha256: String,
    runtime_prefix: std::path::PathBuf,
    symbol: String,
    generation: u64,
    invocation: u64,
    request: serde_json::Value,
    factory_request: serde_json::Value,
}
/// Private worker process entry. Qualification is supplied by its parent over the
/// authenticated channel; this entry cannot issue a native artifact witness.
pub fn run_native_mod_worker(endpoint: &std::path::Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(endpoint)?;
    if !metadata.is_file() || metadata.len() > 65536 {
        bail!("native endpoint file is not bounded regular data");
    }
    let endpoint: WorkerEndpoint = serde_json::from_slice(&std::fs::read(endpoint)?)?;
    let (mut stream, initial) = super::native_channel::connect(endpoint.address, &endpoint.token)?;
    let invocation: WorkerInvocation = serde_json::from_value(initial)?;
    if super::descriptor::native_mod_file_sha256(&invocation.descriptor)? != invocation.descriptor_sha256 {
        bail!("native worker descriptor differs from parent-issued closure");
    }
    let descriptor =
        super::descriptor::read_mod_artifact_descriptor(&invocation.descriptor, &invocation.runtime_prefix)?;
    if !descriptor.registrations.iter().any(|r| r.entry_symbol == invocation.symbol) {
        bail!("native worker symbol is not registered");
    }
    let target = beskid_abi::abi_v5::TargetMetadata::for_triple(&descriptor.target_triple)
        .map_err(|e| anyhow::anyhow!("native target: {e:?}"))?;
    let provider = beskid_abi::runtime_kit::resolve_glue_shared_provider(
        &invocation.runtime_prefix,
        &target,
        descriptor.runtime.profile,
    )
    .map_err(|e| anyhow::anyhow!("native provider: {e}"))?;
    let image_sha = descriptor.files.get(&descriptor.executable_file).context("native executable digest absent")?;
    let images = unsafe {
        beskid_artifacts::native_image::PinnedNativeImages::load(
            &provider.shared_library,
            &descriptor.runtime.shared_library_sha256,
            &descriptor.executable_path(),
            image_sha,
        )
    }?;
    let lease = unsafe { NativeHostLease::open(images) }?;
    let plan_bytes = std::fs::read(descriptor.artifact_dir.join("adapter-plan.json"))?;
    let types = WireTypes::read(&plan_bytes)?;
    let plan: serde_json::Value = serde_json::from_slice(&plan_bytes)?;
    let schema = super::native_correspondence::SyntaxCorrespondence::read(&std::fs::read(
        descriptor.artifact_dir.join("sdk/schema.json"),
    )?)?;
    let entry = entry_from_plan(&plan, &invocation.symbol)?;
    let mut exchange = |request: &serde_json::Value| -> Result<serde_json::Value> {
        super::native_channel::write_frame(&mut stream, &serde_json::json!({"kind":"Service","request":request}))?;
        super::native_channel::read_frame(&mut stream)
    };
    let mut services = super::native_services::WorkerServices {
        plan: &plan,
        types: &types,
        syntax: &schema,
        invocation: invocation.invocation,
        sequence: 1,
        exchange: &mut exchange,
    };
    let result = unsafe {
        invoke_entry(
            &lease,
            0,
            entry,
            &types,
            invocation.generation,
            invocation.invocation,
            &invocation.request,
            &invocation.factory_request,
            &mut |operation, args, arena| services.invoke(operation, args, arena),
        )
    }?;
    drop(services);
    drop(exchange);
    // Heap teardown completes before the parent may consume the final result and
    // before the retained image descriptors can unload.
    drop(lease);
    super::native_channel::write_frame(&mut stream, &serde_json::json!({"kind":"Complete","result":result}))?;
    Ok(())
}
