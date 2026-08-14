use std::collections::BTreeSet;

use super::model::{ParameterLocationV5, RuntimeManifestV5};

pub(super) fn validate(manifest: &RuntimeManifestV5) -> Result<(), String> {
    if manifest.targets.is_empty() {
        return Err("manifest must define at least one target".into());
    }
    unique(manifest.targets.iter().map(|target| target.triple.as_str()), "target")?;
    for target in &manifest.targets {
        if target.triple.is_empty()
            || target.endianness != "little"
            || target.pointer_width != 64
            || target.calling_convention.is_empty()
            || target.object_format.is_empty()
            || target.stack_alignment == 0
            || !matches!(target.symbol_prefix.as_str(), "" | "_")
        {
            return Err(format!("target {} violates generic ABI-v5 target invariants", target.triple));
        }
    }
    let target_names = manifest.targets.iter().map(|target| target.triple.as_str()).collect::<BTreeSet<_>>();
    for service in &manifest.corelib_services {
        let mut binding_targets = BTreeSet::new();
        for binding in &service.target_bindings {
            if !binding_targets.insert(binding.target.as_str()) {
                return Err(format!("duplicate corelib service `{}` target binding", service.name));
            }
        }
        if binding_targets != target_names {
            return Err(format!("corelib service `{}` target bindings are incomplete", service.name));
        }
    }
    let expected_entry_adapters = [
        (
            "x86_64-unknown-linux-gnu",
            "main",
            "utf8_argv",
            "beskid_rt_v5_args_handoff_utf8",
            "args_entry.S",
            &["memcpy", "mmap", "strlen"][..],
        ),
        (
            "aarch64-apple-darwin",
            "main",
            "utf8_argv",
            "beskid_rt_v5_args_handoff_utf8",
            "args_entry.S",
            &["memcpy", "mmap", "strlen"][..],
        ),
        (
            "x86_64-pc-windows-msvc",
            "wmain",
            "utf16_wargv",
            "beskid_rt_v5_args_handoff_utf16",
            "args_entry.asm",
            &["VirtualAlloc"][..],
        ),
    ];
    if manifest.entry_adapters.len() != expected_entry_adapters.len()
        || manifest.entry_adapters.iter().any(|adapter| adapter.name != "Core.Args")
    {
        return Err("Core.Args requires exactly one generated entry adapter per target".into());
    }
    unique(manifest.entry_adapters.iter().map(|adapter| adapter.target.as_str()), "Core.Args entry adapter target")?;
    for (target, executable_entry, capture, handoff, entry_source, imports) in expected_entry_adapters {
        let adapter = manifest
            .entry_adapters
            .iter()
            .find(|adapter| adapter.target == target)
            .ok_or_else(|| format!("missing Core.Args entry adapter for `{target}`"))?;
        if adapter.executable_entry != executable_entry
            || adapter.capture != capture
            || adapter.handoff != handoff
            || adapter.program_entry != "beskid_program_main"
            || adapter.ownership != "process_lifetime_copied_beskid_str_arena"
            || adapter.entry_source != entry_source
            || adapter.os_imports != imports
        {
            return Err(format!("Core.Args entry adapter for `{target}` violates generated provenance"));
        }
    }
    if manifest
        .layouts
        .iter()
        .filter_map(|layout| layout.target.as_deref())
        .any(|target| !target_names.contains(target))
        || manifest.platform_imports.iter().any(|import| !target_names.contains(import.target.as_str()))
    {
        return Err("target-specific contract entry references an unknown target".into());
    }
    if manifest.meta.runtime_publisher != "beskid-lang.org" || manifest.meta.runtime_package != "beskid-runtime-native"
    {
        return Err("canonical runtime package identity is mandatory".into());
    }
    if manifest.meta.trap_exit_status != 101 || manifest.meta.trap_diagnostic != "beskid runtime trap v5" {
        return Err("trap contract must use the stable diagnostic and exit status 101".into());
    }
    let trap =
        manifest.exports.iter().find(|entry| entry.symbol == "beskid_rt_v5_trap").ok_or("missing trap export")?;
    if trap.result != "never" {
        return Err("beskid_rt_v5_trap must be noreturn".into());
    }
    let trap_codes = manifest.traps.iter().map(|trap| trap.code).collect::<BTreeSet<_>>();
    if trap_codes != (1..=10).collect() {
        return Err("trap codes must be exactly 1 through 10".into());
    }
    unique(manifest.traps.iter().map(|trap| trap.name.as_str()), "trap")?;
    if manifest.traps.iter().any(|trap| trap.name.is_empty()) {
        return Err("trap names must not be empty".into());
    }
    unique(manifest.exports.iter().map(|entry| entry.symbol.as_str()), "export")?;
    unique(manifest.intrinsics.iter().map(|entry| entry.name.as_str()), "intrinsic")?;
    unique(manifest.intrinsics.iter().map(|entry| entry.symbol.as_str()), "intrinsic linker symbol")?;
    unique(manifest.soft_builtins.iter().map(|entry| entry.name.as_str()), "soft builtin")?;
    unique(manifest.soft_builtins.iter().map(|entry| entry.symbol.as_str()), "soft builtin linker symbol")?;
    unique(manifest.corelib_services.iter().map(|entry| entry.name.as_str()), "corelib service")?;
    let expected_corelib_services = ["__args_count", "__args_get"].into_iter().collect::<BTreeSet<_>>();
    if let Some(unexpected) = manifest
        .corelib_services
        .iter()
        .map(|service| service.name.as_str())
        .find(|name| !expected_corelib_services.contains(name))
    {
        return Err(format!("unexpected corelib service `{unexpected}`"));
    }
    for (name, adapter, params, result) in [
        ("__args_count", "beskid_rt_v5_args_count", &[][..], "i64"),
        ("__args_get", "beskid_rt_v5_args_get", &["i64"][..], "string"),
    ] {
        let service = manifest
            .corelib_services
            .iter()
            .find(|service| service.name == name)
            .ok_or_else(|| format!("missing corelib service `{name}` adapter binding"))?;
        let actual_params = service.params.iter().map(|param| param.ty.as_str()).collect::<Vec<_>>();
        if service.adapter != adapter || actual_params != params || service.result != result {
            let signature = if params.is_empty() { "[]".to_string() } else { format!("[{}]", params.join(", ")) };
            return Err(format!("corelib service `{name}` signature must be {signature} -> {result}"));
        }
        for binding in &service.target_bindings {
            if binding.implementation != adapter {
                return Err(format!(
                    "corelib service `{name}` binding for `{}` must implement `{adapter}`",
                    binding.target
                ));
            }
        }
    }
    unique(manifest.layouts.iter().map(|entry| (entry.target.as_deref(), entry.name.as_str())), "layout")?;
    for entry in &manifest.exports {
        if entry.symbol.is_empty() || (!entry.symbol.contains("_v5_") && !entry.symbol.ends_with("_v5")) {
            return Err(format!("export {} is not ABI-v5 versioned", entry.symbol));
        }
    }
    let assembly_symbols =
        manifest.assembly.iter().map(|entry| entry.symbol.as_str()).collect::<std::collections::HashSet<_>>();
    for entry in &manifest.intrinsics {
        if entry.name.is_empty()
            || entry.symbol.is_empty()
            || (!entry.symbol.starts_with("beskid_rt_v5_") && !assembly_symbols.contains(entry.symbol.as_str()))
            || !(entry.capability == format!("runtime.bootstrap.{}", entry.name)
                || entry.capability == format!("runtime.adapter.{}", entry.name))
        {
            return Err(format!("intrinsic {} has an invalid capability id", entry.name));
        }
    }
    unique(
        manifest.platform_imports.iter().map(|entry| (entry.target.as_str(), entry.symbol.as_str())),
        "platform import",
    )?;
    for entry in &manifest.platform_imports {
        if entry.symbol.is_empty() || entry.library.is_empty() {
            return Err("platform import symbol/library cannot be empty".into());
        }
    }
    let declared_target_imports = manifest
        .platform_imports
        .iter()
        .map(|entry| (entry.target.as_str(), entry.symbol.as_str()))
        .collect::<BTreeSet<_>>();
    for service in &manifest.corelib_services {
        for binding in &service.target_bindings {
            for import in &binding.os_imports {
                if !declared_target_imports.contains(&(binding.target.as_str(), import.as_str())) {
                    return Err(format!(
                        "corelib service `{}` binding for `{}` names undeclared OS import `{import}`",
                        service.name, binding.target
                    ));
                }
            }
        }
    }
    for adapter in &manifest.entry_adapters {
        for import in &adapter.os_imports {
            if !declared_target_imports.contains(&(adapter.target.as_str(), import.as_str())) {
                return Err(format!(
                    "Core.Args entry adapter for `{}` names undeclared OS import `{import}`",
                    adapter.target
                ));
            }
        }
    }
    let known_types = [
        "void", "never", "pointer", "usize", "isize", "i8", "u8", "i16", "u16", "i32", "u32", "i64", "u64", "v128",
        "f32", "f64", "string",
    ];
    for entry in &manifest.soft_builtins {
        if entry.name.is_empty()
            || !entry.name.starts_with("__")
            || entry.symbol.is_empty()
            || !known_types.contains(&entry.result.as_str())
            || entry.params.iter().any(|param| param.name.is_empty() || !known_types.contains(&param.ty.as_str()))
        {
            return Err(format!("soft builtin `{}` has an invalid declaration", entry.name));
        }
        unique(entry.params.iter().map(|param| param.name.as_str()), "soft builtin parameter")?;
    }
    for (owner, params, result) in manifest
        .exports
        .iter()
        .map(|entry| (entry.symbol.as_str(), entry.params.as_slice(), entry.result.as_str()))
        .chain(
            manifest
                .intrinsics
                .iter()
                .map(|entry| (entry.name.as_str(), entry.params.as_slice(), entry.result.as_str())),
        )
        .chain(
            manifest
                .platform_imports
                .iter()
                .map(|entry| (entry.symbol.as_str(), entry.params.as_slice(), entry.result.as_str())),
        )
    {
        if !known_types.contains(&result)
            || params.iter().any(|param| param.name.is_empty() || !known_types.contains(&param.ty.as_str()))
        {
            return Err(format!("`{owner}` uses an unknown ABI type or unnamed parameter"));
        }
        unique(params.iter().map(|param| param.name.as_str()), "parameter")?;
    }
    for layout in &manifest.layouts {
        if layout.size == 0
            || layout.alignment == 0
            || !layout.alignment.is_power_of_two()
            || layout.size % layout.alignment != 0
        {
            return Err(format!("layout `{}` has invalid size/alignment", layout.name));
        }
        let mut ranges = Vec::new();
        for field in &layout.fields {
            let width =
                abi_width(&field.ty).ok_or_else(|| format!("layout `{}` has unknown field type", layout.name))?;
            if field.offset % width.min(layout.alignment) != 0 || field.offset + width > layout.size {
                return Err(format!("layout `{}` has an invalid field", layout.name));
            }
            ranges.push((field.offset, field.offset + width));
        }
        ranges.sort();
        if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
            return Err(format!("layout `{}` has overlapping fields", layout.name));
        }
    }
    unique(manifest.assembly.iter().map(|entry| entry.symbol.as_str()), "assembly export")?;
    for entry in &manifest.assembly {
        if !entry.symbol.starts_with("beskid_arch_v5_") || entry.params.is_empty() {
            return Err(format!("assembly {} violates generic ABI-v5 invariants", entry.symbol));
        }
        unique(entry.params.iter().map(|param| param.name.as_str()), "assembly parameter")?;
        if entry.preserved.keys().map(String::as_str).collect::<BTreeSet<_>>() != target_names
            || entry.locations.keys().map(String::as_str).collect::<BTreeSet<_>>() != target_names
        {
            return Err(format!("assembly {} target mappings are incomplete", entry.symbol));
        }
        for target in &manifest.targets {
            let locations = &entry.locations[&target.triple];
            if locations.len() != entry.params.len()
                || locations.iter().any(|location| match location {
                    ParameterLocationV5::Register { register } => register.is_empty(),
                    ParameterLocationV5::Stack { base, .. } => base.is_empty(),
                })
                || entry.preserved[&target.triple].iter().any(String::is_empty)
            {
                return Err(format!("assembly {} has an invalid {} mapping", entry.symbol, target.triple));
            }
        }
    }
    if manifest.audit.forbidden_symbol_families.is_empty() {
        return Err("audit policy must define forbidden provenance families".into());
    }
    unique(manifest.audit.forbidden_symbol_families.iter().map(String::as_str), "forbidden provenance family")?;
    Ok(())
}

fn abi_width(ty: &str) -> Option<u64> {
    Some(match ty {
        "i8" | "u8" => 1,
        "i16" | "u16" => 2,
        "i32" | "u32" | "f32" => 4,
        "i64" | "u64" | "f64" | "pointer" | "usize" | "isize" => 8,
        "v128" => 16,
        _ => return None,
    })
}

fn unique<T: Ord>(items: impl IntoIterator<Item = T>, what: &str) -> Result<(), String> {
    let mut set = BTreeSet::new();
    for item in items {
        if !set.insert(item) {
            return Err(format!("duplicate {what}"));
        }
    }
    Ok(())
}
