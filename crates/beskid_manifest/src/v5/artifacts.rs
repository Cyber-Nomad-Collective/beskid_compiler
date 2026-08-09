use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Serialize;

use super::model::{CorelibServiceV5, EntryAdapterV5, GeneratedV5Artifacts, RuntimeManifestV5};
use super::render::{render_asm_target, render_c_header, render_rust};
use super::validation::validate;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeneratedAuditV5<'a> {
    forbidden_symbol_families: &'a [String],
    corelib_services: &'a [CorelibServiceV5],
    entry_adapters: &'a [EntryAdapterV5],
}

pub fn generate_v5_artifacts(manifest: &RuntimeManifestV5) -> Result<GeneratedV5Artifacts, String> {
    validate(manifest)?;
    let manifest = canonicalized(manifest);
    let gnu_asm = manifest
        .targets
        .iter()
        .filter(|target| target.object_format != "coff")
        .map(|target| (target.triple.clone(), render_asm_target(&manifest, target, false)))
        .collect::<BTreeMap<_, _>>();
    let masm = manifest
        .targets
        .iter()
        .filter(|target| target.object_format == "coff")
        .map(|target| (target.triple.clone(), render_asm_target(&manifest, target, true)))
        .collect::<BTreeMap<_, _>>();
    Ok(GeneratedV5Artifacts {
        rust: render_rust(&manifest, &gnu_asm, &masm),
        c_header: render_c_header(&manifest),
        gnu_asm,
        masm,
        abi_json: canonical_json(&manifest)?,
        audit_json: canonical_json(&GeneratedAuditV5 {
            forbidden_symbol_families: &manifest.audit.forbidden_symbol_families,
            corelib_services: &manifest.corelib_services,
            entry_adapters: &manifest.entry_adapters,
        })?,
    })
}

pub fn write_v5_artifacts(manifest: &RuntimeManifestV5, workspace: &Path) -> Result<(), String> {
    let artifacts = generate_v5_artifacts(manifest)?;
    let generated = workspace.join("crates/beskid_abi/src/generated");
    let include = workspace.join("crates/beskid_abi/include");
    fs::create_dir_all(&generated).map_err(|error| error.to_string())?;
    fs::create_dir_all(&include).map_err(|error| error.to_string())?;
    for (path, contents) in [
        (generated.join("abi_v5_contract.rs"), artifacts.rust),
        (include.join("beskid_runtime_abi_v5.h"), artifacts.c_header),
        (include.join("abi-v5.json"), artifacts.abi_json),
        (include.join("abi-v5-audit.json"), artifacts.audit_json),
    ] {
        fs::write(path, contents).map_err(|error| error.to_string())?;
    }
    for (target, contents) in artifacts.gnu_asm.into_iter().chain(artifacts.masm) {
        fs::write(include.join(format!("beskid_runtime_abi_v5_{}.inc", target.replace('-', "_"))), contents)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn canonical_json(value: &impl Serialize) -> Result<String, String> {
    let mut output = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    output.push('\n');
    Ok(output)
}

fn canonicalized(manifest: &RuntimeManifestV5) -> RuntimeManifestV5 {
    let mut value = manifest.clone();
    value.targets.sort_by(|a, b| a.triple.cmp(&b.triple));
    value.exports.sort_by(|a, b| a.symbol.cmp(&b.symbol));
    value.intrinsics.sort_by(|a, b| a.name.cmp(&b.name));
    value.soft_builtins.sort_by(|a, b| a.name.cmp(&b.name));
    value.layouts.sort_by(|a, b| a.target.cmp(&b.target).then_with(|| a.name.cmp(&b.name)));
    for layout in &mut value.layouts {
        layout.fields.sort_by_key(|field| field.offset);
    }
    value.platform_imports.sort_by(|a, b| a.target.cmp(&b.target).then_with(|| a.symbol.cmp(&b.symbol)));
    value.corelib_services.sort_by(|a, b| a.name.cmp(&b.name));
    for service in &mut value.corelib_services {
        service.target_bindings.sort_by(|a, b| a.target.cmp(&b.target));
        for binding in &mut service.target_bindings {
            binding.os_imports.sort();
        }
    }
    value.entry_adapters.sort_by(|a, b| a.target.cmp(&b.target));
    for adapter in &mut value.entry_adapters {
        adapter.os_imports.sort();
    }
    value.assembly.sort_by(|a, b| a.symbol.cmp(&b.symbol));
    value.traps.sort_by_key(|trap| trap.code);
    value.audit.forbidden_symbol_families.sort();
    value
}
