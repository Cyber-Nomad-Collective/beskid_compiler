use std::collections::{BTreeMap, BTreeSet};

use bsol::{BsolBlock, BsolItem, BsolListItem, BsolValue, parse_bsol_document};

use super::model::{
    AssemblyV5, AuditV5, CorelibServiceV5, EntryAdapterV5, FieldV5, FunctionV5, IntrinsicV5, LayoutV5,
    ParameterLocationV5, ParameterV5, PlatformImportV5, RuntimeManifestV5, RuntimeMetaV5, SoftBuiltinV5,
    StatusV5, StatusValueV5, TargetAdapterBindingV5, TargetV5, TrapV5,
};
use super::validation::validate;

pub fn load_v5_manifest_source(source: &str) -> Result<RuntimeManifestV5, String> {
    let document = parse_bsol_document(source).map_err(|error| error.to_string())?;
    let allowed_blocks = [
        "manifest",
        "target",
        "export",
        "intrinsic",
        "soft_builtin",
        "layout",
        "platform_import",
        "corelib_service",
        "entry_adapter",
        "assembly",
        "trap",
        "status",
        "audit",
    ];
    if let Some(block) = document.blocks.iter().find(|block| !allowed_blocks.contains(&block.kind.as_str())) {
        return Err(format!("unknown top-level block `{}`", block.kind));
    }
    let manifest = one(&document.blocks, "manifest")?;
    ensure_fields(
        manifest,
        &[
            "abi_version",
            "schema_version",
            "runtime_publisher",
            "runtime_package",
            "trap_exit_status",
            "trap_diagnostic",
            "build_profiles",
        ],
    )?;
    let meta = RuntimeMetaV5 {
        abi_version: u32_field(manifest, "abi_version")?,
        schema_version: u32_field(manifest, "schema_version")?,
        runtime_publisher: string_field(manifest, "runtime_publisher")?,
        runtime_package: string_field(manifest, "runtime_package")?,
        trap_exit_status: u32_field(manifest, "trap_exit_status")?,
        trap_diagnostic: string_field(manifest, "trap_diagnostic")?,
    };
    if meta.abi_version != 5 || meta.schema_version != 2 {
        return Err("runtime manifest must be ABI 5 schema 2".into());
    }
    let targets = blocks(&document.blocks, "target")
        .map(|block| {
            ensure_fields(
                block,
                &[
                    "endianness",
                    "pointer_width",
                    "calling_convention",
                    "object_format",
                    "symbol_prefix",
                    "stack_alignment",
                    "shadow_space",
                ],
            )?;
            Ok(TargetV5 {
                triple: label(block)?,
                endianness: string_field(block, "endianness")?,
                pointer_width: u32_field(block, "pointer_width")?,
                calling_convention: string_field(block, "calling_convention")?,
                object_format: string_field(block, "object_format")?,
                symbol_prefix: string_field(block, "symbol_prefix")?,
                stack_alignment: u32_field(block, "stack_alignment")?,
                shadow_space: u32_field(block, "shadow_space")?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let exports = blocks(&document.blocks, "export")
        .map(|block| {
            ensure_fields(block, &["params", "returns"])?;
            function(block)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let intrinsics = blocks(&document.blocks, "intrinsic")
        .map(|block| {
            ensure_fields(block, &["symbol", "capability", "params", "returns", "result_status", "target_bindings"])?;
            Ok(IntrinsicV5 {
                name: label(block)?,
                symbol: string_field(block, "symbol")?,
                capability: string_field(block, "capability")?,
                params: parameters(block, "params")?,
                result: string_field(block, "returns")?,
                result_status: optional_string_field(block, "result_status")?,
                target_bindings: target_adapter_bindings(block)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let soft_builtins = blocks(&document.blocks, "soft_builtin")
        .map(|block| {
            ensure_fields(block, &["symbol", "params", "returns"])?;
            Ok(SoftBuiltinV5 {
                name: label(block)?,
                symbol: string_field(block, "symbol")?,
                params: parameters(block, "params")?,
                result: string_field(block, "returns")?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let layouts = blocks(&document.blocks, "layout")
        .map(|block| {
            ensure_fields(block, &["target", "size", "alignment", "fields", "project_to_runtime"])?;
            Ok(LayoutV5 {
                name: label(block)?,
                target: optional_string_field(block, "target")?,
                size: u64_field(block, "size")?,
                alignment: u64_field(block, "alignment")?,
                fields: fields(block)?,
                project_to_runtime: optional_string_field(block, "project_to_runtime")?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let platform_imports = blocks(&document.blocks, "platform_import")
        .map(|block| {
            ensure_fields(block, &["target", "library", "params", "returns"])?;
            Ok(PlatformImportV5 {
                symbol: label(block)?,
                target: string_field(block, "target")?,
                library: string_field(block, "library")?,
                params: parameters(block, "params")?,
                result: string_field(block, "returns")?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let corelib_services = blocks(&document.blocks, "corelib_service")
        .map(|block| {
            ensure_fields(block, &["adapter", "params", "returns", "target_bindings"])?;
            Ok(CorelibServiceV5 {
                name: label(block)?,
                adapter: string_field(block, "adapter")?,
                params: parameters(block, "params")?,
                result: string_field(block, "returns")?,
                target_bindings: target_adapter_bindings(block)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let entry_adapters = blocks(&document.blocks, "entry_adapter")
        .map(|block| {
            ensure_fields(
                block,
                &[
                    "target",
                    "executable_entry",
                    "program_entry",
                    "capture",
                    "handoff",
                    "ownership",
                    "entry_source",
                    "os_imports",
                ],
            )?;
            Ok(EntryAdapterV5 {
                name: label(block)?,
                target: string_field(block, "target")?,
                executable_entry: string_field(block, "executable_entry")?,
                program_entry: string_field(block, "program_entry")?,
                capture: string_field(block, "capture")?,
                handoff: string_field(block, "handoff")?,
                ownership: string_field(block, "ownership")?,
                entry_source: string_field(block, "entry_source")?,
                os_imports: list_field(block, "os_imports")?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let assembly = blocks(&document.blocks, "assembly")
        .map(|block| parse_assembly(block, &targets))
        .collect::<Result<Vec<_>, _>>()?;
    let traps = blocks(&document.blocks, "trap")
        .map(|block| {
            ensure_fields(block, &["code"])?;
            Ok(TrapV5 { name: label(block)?, code: u32_field(block, "code")? })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let statuses = blocks(&document.blocks, "status")
        .map(|block| {
            ensure_fields(block, &["repr", "values"])?;
            let values = list_items(block, "values")?
                .iter()
                .map(|item| match item {
                    BsolListItem::InlineMap(map) => {
                        ensure_map_fields(map, &["name", "value"])?;
                        Ok(StatusValueV5 {
                            name: map_string(map, "name")?,
                            value: map_string(map, "value")?.parse().map_err(|_| "status value must be integer")?,
                        })
                    }
                    _ => Err("status values entries must be inline maps".into()),
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(StatusV5 { name: label(block)?, repr: string_field(block, "repr")?, values })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let audit_block = one(&document.blocks, "audit")?;
    ensure_fields(audit_block, &["forbidden_symbol_families"])?;
    let audit = AuditV5 { forbidden_symbol_families: list_field(audit_block, "forbidden_symbol_families")? };
    let result = RuntimeManifestV5 {
        meta,
        targets,
        exports,
        intrinsics,
        soft_builtins,
        layouts,
        platform_imports,
        corelib_services,
        entry_adapters,
        assembly,
        traps,
        statuses,
        audit,
    };
    validate(&result)?;
    Ok(result)
}

fn one<'a>(blocks: &'a [BsolBlock], kind: &str) -> Result<&'a BsolBlock, String> {
    let mut found = blocks.iter().filter(|block| block.kind == kind);
    let first = found.next().ok_or_else(|| format!("missing `{kind}` block"))?;
    if found.next().is_some() { Err(format!("duplicate `{kind}` block")) } else { Ok(first) }
}
fn blocks<'a>(blocks: &'a [BsolBlock], kind: &'a str) -> impl Iterator<Item = &'a BsolBlock> {
    blocks.iter().filter(move |block| block.kind == kind)
}
fn label(block: &BsolBlock) -> Result<String, String> {
    block.label.as_ref().map(|label| label.value.clone()).ok_or_else(|| format!("`{}` requires a label", block.kind))
}
fn value<'a>(block: &'a BsolBlock, key: &str) -> Result<&'a BsolValue, String> {
    block
        .items
        .iter()
        .find_map(|item| match item {
            BsolItem::Assignment(entry) if entry.key == key => Some(&entry.value),
            _ => None,
        })
        .ok_or_else(|| format!("`{}` missing `{key}`", block.kind))
}
fn ensure_fields(block: &BsolBlock, allowed: &[&str]) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for item in &block.items {
        match item {
            BsolItem::Assignment(entry) => {
                if !allowed.contains(&entry.key.as_str()) {
                    return Err(format!("unknown field `{}` in `{}`", entry.key, block.kind));
                }
                if !seen.insert(entry.key.as_str()) {
                    return Err(format!("duplicate field `{}` in `{}`", entry.key, block.kind));
                }
            }
            BsolItem::Block(_) => {
                return Err(format!("nested blocks are forbidden in `{}`", block.kind));
            }
        }
    }
    Ok(())
}
fn string_value(value: &BsolValue) -> Result<String, String> {
    match value {
        BsolValue::Ident(value) => Ok(value.clone()),
        BsolValue::QuotedString(value) => Ok(value.value.clone()),
        _ => Err("expected identifier or quoted string".into()),
    }
}
fn string_field(block: &BsolBlock, key: &str) -> Result<String, String> {
    string_value(value(block, key)?)
}
fn optional_string_field(block: &BsolBlock, key: &str) -> Result<Option<String>, String> {
    block
        .items
        .iter()
        .find_map(|item| match item {
            BsolItem::Assignment(entry) if entry.key == key => Some(&entry.value),
            _ => None,
        })
        .map(string_value)
        .transpose()
}
fn u32_field(block: &BsolBlock, key: &str) -> Result<u32, String> {
    string_field(block, key)?.parse().map_err(|_| format!("`{key}` must be u32"))
}
fn u64_field(block: &BsolBlock, key: &str) -> Result<u64, String> {
    string_field(block, key)?.parse().map_err(|_| format!("`{key}` must be u64"))
}
fn list_items<'a>(block: &'a BsolBlock, key: &str) -> Result<&'a [BsolListItem], String> {
    match value(block, key)? {
        BsolValue::BracketList(list) => Ok(&list.items),
        _ => Err(format!("`{key}` must be a list")),
    }
}
fn list_field(block: &BsolBlock, key: &str) -> Result<Vec<String>, String> {
    list_items(block, key)?
        .iter()
        .map(|item| match item {
            BsolListItem::Ident(value) => Ok(value.clone()),
            BsolListItem::QuotedString(value) => Ok(value.value.clone()),
            _ => Err(format!("`{key}` entries must be identifiers or strings")),
        })
        .collect()
}
fn parameters(block: &BsolBlock, key: &str) -> Result<Vec<ParameterV5>, String> {
    list_items(block, key)?
        .iter()
        .map(|item| match item {
            BsolListItem::InlineMap(map) => {
                ensure_map_fields(map, &["name", "type"])?;
                Ok(ParameterV5 { name: map_string(map, "name")?, ty: map_string(map, "type")? })
            }
            _ => Err(format!("`{key}` entries must be inline maps")),
        })
        .collect()
}

fn target_adapter_bindings(block: &BsolBlock) -> Result<Vec<TargetAdapterBindingV5>, String> {
    let Some(items) = optional_list_items(block, "target_bindings")? else {
        return Ok(Vec::new());
    };
    items
        .iter()
        .map(|item| match item {
            BsolListItem::InlineMap(map) => {
                ensure_map_fields(map, &["target", "implementation", "os_imports"])?;
                Ok(TargetAdapterBindingV5 {
                    target: map_string(map, "target")?,
                    implementation: map_string(map, "implementation")?,
                    os_imports: map_string_list(map, "os_imports")?,
                })
            }
            _ => Err("`target_bindings` entries must be inline maps".into()),
        })
        .collect()
}

fn optional_list_items<'a>(
    block: &'a BsolBlock,
    key: &str,
) -> Result<Option<&'a [BsolListItem]>, String> {
    let Some(BsolItem::Assignment(assignment)) = block.items.iter().find_map(|item| match item {
        BsolItem::Assignment(entry) if entry.key == key => Some(item),
        _ => None,
    }) else {
        return Ok(None);
    };
    let BsolValue::BracketList(list) = &assignment.value else {
        return Err(format!("`{key}` must be a list"));
    };
    Ok(Some(&list.items))
}
fn fields(block: &BsolBlock) -> Result<Vec<FieldV5>, String> {
    list_items(block, "fields")?
        .iter()
        .map(|item| match item {
            BsolListItem::InlineMap(map) => {
                ensure_map_fields(map, &["name", "offset", "type"])?;
                Ok(FieldV5 {
                    name: map_string(map, "name")?,
                    offset: map_string(map, "offset")?.parse().map_err(|_| "field offset must be u64")?,
                    ty: map_string(map, "type")?,
                })
            }
            _ => Err("fields entries must be inline maps".into()),
        })
        .collect()
}
fn map_string(map: &bsol::BsolInlineMap, key: &str) -> Result<String, String> {
    map.entries
        .iter()
        .find(|entry| entry.key == key)
        .ok_or_else(|| format!("inline map missing `{key}`"))
        .and_then(|entry| string_value(&entry.value))
}

fn map_string_list(map: &bsol::BsolInlineMap, key: &str) -> Result<Vec<String>, String> {
    let value =
        map.entries.iter().find(|entry| entry.key == key).ok_or_else(|| format!("inline map missing `{key}`"))?;
    let BsolValue::BracketList(list) = &value.value else {
        return Err(format!("inline map `{key}` must be a list"));
    };
    list.items
        .iter()
        .map(|item| match item {
            BsolListItem::Ident(value) => Ok(value.clone()),
            BsolListItem::QuotedString(value) => Ok(value.value.clone()),
            _ => Err(format!("inline map `{key}` entries must be identifiers or strings")),
        })
        .collect()
}
fn ensure_map_fields(map: &bsol::BsolInlineMap, allowed: &[&str]) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for entry in &map.entries {
        if !allowed.contains(&entry.key.as_str()) {
            return Err(format!("unknown inline-map field `{}`", entry.key));
        }
        if !seen.insert(entry.key.as_str()) {
            return Err(format!("duplicate inline-map field `{}`", entry.key));
        }
    }
    Ok(())
}
fn function(block: &BsolBlock) -> Result<FunctionV5, String> {
    Ok(FunctionV5 {
        symbol: label(block)?,
        params: parameters(block, "params")?,
        result: string_field(block, "returns")?,
    })
}
fn parse_assembly(block: &BsolBlock, targets: &[TargetV5]) -> Result<AssemblyV5, String> {
    let mut allowed = vec!["params".to_string(), "returns".to_string()];
    for target in targets {
        let slug = target.triple.replace('-', "_");
        allowed.push(format!("{slug}_preserved"));
        allowed.push(format!("{slug}_locations"));
    }
    ensure_fields(block, &allowed.iter().map(String::as_str).collect::<Vec<_>>())?;
    let mut preserved = BTreeMap::new();
    let mut locations = BTreeMap::new();
    for target in targets {
        let slug = target.triple.replace('-', "_");
        preserved.insert(target.triple.clone(), list_field(block, &format!("{slug}_preserved"))?);
        locations.insert(target.triple.clone(), parameter_locations(block, &format!("{slug}_locations"))?);
    }
    Ok(AssemblyV5 {
        symbol: label(block)?,
        params: parameters(block, "params")?,
        result: string_field(block, "returns")?,
        preserved,
        locations,
    })
}

fn parameter_locations(block: &BsolBlock, key: &str) -> Result<Vec<ParameterLocationV5>, String> {
    list_items(block, key)?
        .iter()
        .map(|item| match item {
            BsolListItem::InlineMap(map) => {
                let has_register = map.entries.iter().any(|entry| entry.key == "register");
                let has_stack =
                    map.entries.iter().any(|entry| entry.key == "stack_base" || entry.key == "stack_offset");
                match (has_register, has_stack) {
                    (true, false) => {
                        ensure_map_fields(map, &["register"])?;
                        Ok(ParameterLocationV5::Register { register: map_string(map, "register")? })
                    }
                    (false, true) => {
                        ensure_map_fields(map, &["stack_base", "stack_offset"])?;
                        let offset = map_string(map, "stack_offset")?
                            .parse::<u64>()
                            .map_err(|_| "`stack_offset` must be u64")?;
                        Ok(ParameterLocationV5::Stack { base: map_string(map, "stack_base")?, offset })
                    }
                    _ => Err("parameter location must be exactly register or stack".into()),
                }
            }
            _ => Err(format!("`{key}` entries must be typed inline maps")),
        })
        .collect()
}
