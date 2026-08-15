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
    let expected_corelib_services = [
        "__array_new",
        "__array_len",
        "__bytes_compare",
        "__bytes_copy",
        "__bytes_from_str",
        "__bytes_get",
        "__bytes_set",
        "__str_new",
        "__str_len",
        "__str_eq",
        "__str_concat",
        "__str_from_i64",
        "__str_slice",
        "__str_from_bytes_utf8",
        "__dynamic_cast_checked",
        "__dynamic_cell_create",
        "__dynamic_cell_wrap",
        "__dynamic_map_aot",
        "__dynamic_map_fallback",
        "__dynamic_object_alloc",
        "__fiber_spawn",
        "__fiber_cancel",
        "__fiber_detach",
        "__fiber_join_status",
        "__fiber_join_value",
        "__fiber_now_millis",
        "__fiber_processor_count",
        "__fiber_current_id",
        "__runtime_preempt_check",
        "__beskid_register_callbacks",
        "__beskid_register_handlers",
        "__clock_monotonic_nanos",
        "__clock_realtime_nanos",
        "__composition_container_create",
        "__composition_container_drop",
        "__composition_slot_store",
        "__composition_launch",
        "__composition_scope_enter",
        "__composition_scope_leave",
        "__composition_scope_depth",
        "__composition_shutdown",
        "__process_exit",
        "__process_getpid",
        "__env_get",
        "__env_set",
        "__env_getcwd",
        "__tty_winsize",
        "__syscall_read",
        "__syscall_read_bytes",
        "__syscall_write",
        "__syscall_write_bytes",
        "__panic",
        "__panic_str",
        "__alloc",
        "__gc_write_barrier",
        "__gc_bytes_allocated",
        "__gc_object_count",
        "__gc_phase",
        "__gc_collect",
        "__gc_collect_if_needed",
        "__gc_register_root",
        "__gc_unregister_root",
        "__gc_external_root_count",
        "__gc_root_handle",
        "__gc_unroot_handle",
        "__event_get_handler",
        "__event_len",
        "__event_subscribe",
        "__event_unsubscribe_first",
        "__hub_create",
        "__hub_register",
        "__hub_unregister",
        "__hub_wait_receive_status",
        "__hub_wait_receive_value",
        "__hub_wait_receive_index",
        "__channel_create",
        "__channel_send",
        "__channel_try_send",
        "__channel_receive_status",
        "__channel_receive_value",
        "__channel_try_receive",
        "__channel_close",
        "__channel_send_ptr",
        "__channel_try_send_ptr",
        "__channel_receive_ptr",
        "__channel_try_receive_ptr",
        "__mutex_create",
        "__mutex_lock",
        "__mutex_try_lock",
        "__mutex_unlock",
        "__wait_group_create",
        "__wait_group_add",
        "__wait_group_done",
        "__wait_group_wait",
        "__fs_read_text",
        "__fs_write_text",
        "__fs_exists",
        "__fs_mkdir",
        "__fs_delete",
        "__args_count",
        "__args_get",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    let actual_corelib_services =
        manifest.corelib_services.iter().map(|service| service.name.as_str()).collect::<BTreeSet<_>>();
    if actual_corelib_services != expected_corelib_services {
        return Err("corelib services must match the canonical runtime service set".into());
    }
    for service in &manifest.corelib_services {
        let expected_adapter = service
            .name
            .strip_prefix("__")
            .ok_or_else(|| format!("corelib service `{}` must use a compiler-owned name", service.name))?;
        if service.adapter != expected_adapter
            && !matches!(
                service.name.as_str(),
                "__args_count"
                    | "__args_get"
                    | "__fs_read_text"
                    | "__fs_write_text"
                    | "__fs_exists"
                    | "__fs_mkdir"
                    | "__fs_delete"
            )
        {
            return Err(format!("corelib service `{}` must use canonical adapter `{expected_adapter}`", service.name));
        }
        if service.target_bindings.iter().any(|binding| binding.implementation != service.adapter) {
            return Err(format!(
                "corelib service `{}` target binding must implement its canonical adapter",
                service.name
            ));
        }
    }
    for (name, adapter, params, result) in [
        ("__args_count", "beskid_rt_v5_args_count", &[][..], "i64"),
        ("__args_get", "beskid_rt_v5_args_get", &["i64"][..], "string"),
        ("__fs_read_text", "beskid_rt_v5_fs_read_text", &["pointer", "pointer"][..], "i32"),
        ("__fs_write_text", "beskid_rt_v5_fs_write_text", &["pointer", "pointer"][..], "i32"),
        ("__fs_exists", "beskid_rt_v5_fs_exists", &["pointer"][..], "i32"),
        ("__fs_mkdir", "beskid_rt_v5_fs_mkdir", &["pointer"][..], "i32"),
        ("__fs_delete", "beskid_rt_v5_fs_delete", &["pointer"][..], "i32"),
    ] {
        let service = manifest
            .corelib_services
            .iter()
            .find(|service| service.name == name)
            .ok_or_else(|| format!("missing canonical corelib service `{name}` adapter binding"))?;
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
                || entry.capability == format!("runtime.adapter.{}", entry.name)
                || entry.capability == format!("runtime.scheduler.{}", entry.name))
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
        let layout_sizes: std::collections::BTreeMap<&str, u64> =
            manifest.layouts.iter().map(|entry| (entry.name.as_str(), entry.size)).collect();
        let mut ranges = Vec::new();
        for field in &layout.fields {
            let (element_width, count) = field_dimensions(&field.ty, &layout_sizes)
                .ok_or_else(|| format!("layout `{}` has unknown field type", layout.name))?;
            let width = element_width * count;
            if field.offset % element_width.min(layout.alignment) != 0 || field.offset + width > layout.size {
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

fn field_dimensions(ty: &str, layout_sizes: &std::collections::BTreeMap<&str, u64>) -> Option<(u64, u64)> {
    if let Some(end) = ty.rfind(']') {
        let start = ty.rfind('[')?;
        let element = &ty[..start];
        let count: u64 = ty[start + 1..end].parse().ok()?;
        let element_width = abi_width(element).or_else(|| layout_sizes.get(element).copied())?;
        return Some((element_width, count));
    }
    let element_width = abi_width(ty).or_else(|| layout_sizes.get(ty).copied())?;
    Some((element_width, 1))
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
