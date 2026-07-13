//! ABI-v5 runtime manifest parsing and deterministic multi-target generation.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use bsol::{BsolBlock, BsolItem, BsolListItem, BsolValue, parse_bsol_document};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeMetaV5 {
    pub abi_version: u32,
    pub schema_version: u32,
    pub runtime_publisher: String,
    pub runtime_package: String,
    pub trap_exit_status: u32,
    pub trap_diagnostic: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetV5 {
    pub triple: String,
    pub endianness: String,
    pub pointer_width: u32,
    pub calling_convention: String,
    pub object_format: String,
    pub symbol_prefix: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ParameterV5 {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FunctionV5 {
    pub symbol: String,
    pub params: Vec<ParameterV5>,
    pub result: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IntrinsicV5 {
    pub name: String,
    pub capability: String,
    pub params: Vec<ParameterV5>,
    pub result: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldV5 {
    pub name: String,
    pub offset: u64,
    #[serde(rename = "type")]
    pub ty: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LayoutV5 {
    pub name: String,
    pub target: Option<String>,
    pub size: u64,
    pub alignment: u64,
    pub fields: Vec<FieldV5>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlatformImportV5 {
    pub symbol: String,
    pub target: String,
    pub library: String,
    pub params: Vec<ParameterV5>,
    pub result: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AssemblyV5 {
    pub symbol: String,
    pub params: Vec<ParameterV5>,
    pub result: String,
    pub preserved: BTreeMap<String, Vec<String>>,
    pub locations: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrapV5 {
    pub name: String,
    pub code: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditV5 {
    pub forbidden_symbol_families: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeManifestV5 {
    pub meta: RuntimeMetaV5,
    pub targets: Vec<TargetV5>,
    pub exports: Vec<FunctionV5>,
    pub intrinsics: Vec<IntrinsicV5>,
    pub layouts: Vec<LayoutV5>,
    pub platform_imports: Vec<PlatformImportV5>,
    pub assembly: Vec<AssemblyV5>,
    pub traps: Vec<TrapV5>,
    pub audit: AuditV5,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedV5Artifacts {
    pub rust: String,
    pub c_header: String,
    pub gnu_asm: BTreeMap<String, String>,
    pub masm: BTreeMap<String, String>,
    pub abi_json: String,
    pub audit_json: String,
}

pub fn load_v5_manifest_source(source: &str) -> Result<RuntimeManifestV5, String> {
    let document = parse_bsol_document(source).map_err(|error| error.to_string())?;
    let allowed_blocks = [
        "manifest",
        "target",
        "export",
        "intrinsic",
        "layout",
        "platform_import",
        "assembly",
        "trap",
        "audit",
    ];
    if let Some(block) = document
        .blocks
        .iter()
        .find(|block| !allowed_blocks.contains(&block.kind.as_str()))
    {
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
    if meta.abi_version != 5 || meta.schema_version != 1 {
        return Err("runtime manifest must be ABI 5 schema 1".into());
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
                ],
            )?;
            Ok(TargetV5 {
                triple: label(block)?,
                endianness: string_field(block, "endianness")?,
                pointer_width: u32_field(block, "pointer_width")?,
                calling_convention: string_field(block, "calling_convention")?,
                object_format: string_field(block, "object_format")?,
                symbol_prefix: string_field(block, "symbol_prefix")?,
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
            ensure_fields(block, &["capability", "params", "returns"])?;
            Ok(IntrinsicV5 {
                name: label(block)?,
                capability: string_field(block, "capability")?,
                params: parameters(block, "params")?,
                result: string_field(block, "returns")?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let layouts = blocks(&document.blocks, "layout")
        .map(|block| {
            ensure_fields(block, &["target", "size", "alignment", "fields"])?;
            Ok(LayoutV5 {
                name: label(block)?,
                target: optional_string_field(block, "target")?,
                size: u64_field(block, "size")?,
                alignment: u64_field(block, "alignment")?,
                fields: fields(block)?,
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
    let assembly = blocks(&document.blocks, "assembly")
        .map(parse_assembly)
        .collect::<Result<Vec<_>, _>>()?;
    let traps = blocks(&document.blocks, "trap")
        .map(|block| {
            ensure_fields(block, &["code"])?;
            Ok(TrapV5 {
                name: label(block)?,
                code: u32_field(block, "code")?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let audit_block = one(&document.blocks, "audit")?;
    ensure_fields(audit_block, &["forbidden_symbol_families"])?;
    let audit = AuditV5 {
        forbidden_symbol_families: list_field(audit_block, "forbidden_symbol_families")?,
    };
    let result = RuntimeManifestV5 {
        meta,
        targets,
        exports,
        intrinsics,
        layouts,
        platform_imports,
        assembly,
        traps,
        audit,
    };
    validate(&result)?;
    Ok(result)
}

fn validate(manifest: &RuntimeManifestV5) -> Result<(), String> {
    if manifest.targets.is_empty() {
        return Err("manifest must define at least one target".into());
    }
    unique(
        manifest.targets.iter().map(|target| target.triple.as_str()),
        "target",
    )?;
    for target in &manifest.targets {
        if target.triple.is_empty()
            || target.endianness != "little"
            || target.pointer_width != 64
            || target.calling_convention.is_empty()
            || target.object_format.is_empty()
            || !matches!(target.symbol_prefix.as_str(), "" | "_")
        {
            return Err(format!(
                "target {} violates generic ABI-v5 target invariants",
                target.triple
            ));
        }
    }
    let target_names = manifest
        .targets
        .iter()
        .map(|target| target.triple.as_str())
        .collect::<BTreeSet<_>>();
    if manifest
        .layouts
        .iter()
        .filter_map(|layout| layout.target.as_deref())
        .any(|target| !target_names.contains(target))
        || manifest
            .platform_imports
            .iter()
            .any(|import| !target_names.contains(import.target.as_str()))
    {
        return Err("target-specific contract entry references an unknown target".into());
    }
    if manifest.meta.runtime_publisher != "beskid-lang.org"
        || manifest.meta.runtime_package != "beskid-runtime-native"
    {
        return Err("canonical runtime package identity is mandatory".into());
    }
    if manifest.meta.trap_exit_status != 101
        || manifest.meta.trap_diagnostic != "beskid runtime trap v5"
    {
        return Err("trap contract must use the stable diagnostic and exit status 101".into());
    }
    let trap = manifest
        .exports
        .iter()
        .find(|entry| entry.symbol == "beskid_rt_v5_trap")
        .ok_or("missing trap export")?;
    if trap.result != "never" {
        return Err("beskid_rt_v5_trap must be noreturn".into());
    }
    let trap_codes = manifest
        .traps
        .iter()
        .map(|trap| trap.code)
        .collect::<BTreeSet<_>>();
    if trap_codes != (1..=10).collect() {
        return Err("trap codes must be exactly 1 through 10".into());
    }
    unique(manifest.traps.iter().map(|trap| trap.name.as_str()), "trap")?;
    if manifest.traps.iter().any(|trap| trap.name.is_empty()) {
        return Err("trap names must not be empty".into());
    }
    unique(
        manifest.exports.iter().map(|entry| entry.symbol.as_str()),
        "export",
    )?;
    unique(
        manifest.intrinsics.iter().map(|entry| entry.name.as_str()),
        "intrinsic",
    )?;
    unique(
        manifest
            .layouts
            .iter()
            .map(|entry| (entry.target.as_deref(), entry.name.as_str())),
        "layout",
    )?;
    for entry in &manifest.exports {
        if entry.symbol.is_empty()
            || (!entry.symbol.contains("_v5_") && !entry.symbol.ends_with("_v5"))
        {
            return Err(format!("export {} is not ABI-v5 versioned", entry.symbol));
        }
    }
    for entry in &manifest.intrinsics {
        if entry.name.is_empty() || entry.capability != format!("runtime.bootstrap.{}", entry.name)
        {
            return Err(format!(
                "intrinsic {} has an invalid capability id",
                entry.name
            ));
        }
    }
    unique(
        manifest
            .platform_imports
            .iter()
            .map(|entry| (entry.target.as_str(), entry.symbol.as_str())),
        "platform import",
    )?;
    for entry in &manifest.platform_imports {
        if entry.symbol.is_empty() || entry.library.is_empty() {
            return Err("platform import symbol/library cannot be empty".into());
        }
    }
    let known_types = [
        "void", "never", "pointer", "usize", "isize", "i8", "u8", "i16", "u16", "i32", "u32",
        "i64", "u64", "v128", "f32", "f64",
    ];
    for (owner, params, result) in manifest
        .exports
        .iter()
        .map(|entry| {
            (
                entry.symbol.as_str(),
                entry.params.as_slice(),
                entry.result.as_str(),
            )
        })
        .chain(manifest.intrinsics.iter().map(|entry| {
            (
                entry.name.as_str(),
                entry.params.as_slice(),
                entry.result.as_str(),
            )
        }))
        .chain(manifest.platform_imports.iter().map(|entry| {
            (
                entry.symbol.as_str(),
                entry.params.as_slice(),
                entry.result.as_str(),
            )
        }))
    {
        if !known_types.contains(&result)
            || params
                .iter()
                .any(|param| param.name.is_empty() || !known_types.contains(&param.ty.as_str()))
        {
            return Err(format!(
                "`{owner}` uses an unknown ABI type or unnamed parameter"
            ));
        }
        unique(params.iter().map(|param| param.name.as_str()), "parameter")?;
    }
    for layout in &manifest.layouts {
        if layout.size == 0
            || layout.alignment == 0
            || !layout.alignment.is_power_of_two()
            || layout.size % layout.alignment != 0
        {
            return Err(format!(
                "layout `{}` has invalid size/alignment",
                layout.name
            ));
        }
        let mut ranges = Vec::new();
        for field in &layout.fields {
            let width = abi_width(&field.ty)
                .ok_or_else(|| format!("layout `{}` has unknown field type", layout.name))?;
            if field.offset % width.min(layout.alignment) != 0 || field.offset + width > layout.size
            {
                return Err(format!("layout `{}` has an invalid field", layout.name));
            }
            ranges.push((field.offset, field.offset + width));
        }
        ranges.sort();
        if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
            return Err(format!("layout `{}` has overlapping fields", layout.name));
        }
    }
    unique(
        manifest.assembly.iter().map(|entry| entry.symbol.as_str()),
        "assembly export",
    )?;
    for entry in &manifest.assembly {
        if !entry.symbol.starts_with("beskid_arch_v5_") || entry.params.is_empty() {
            return Err(format!(
                "assembly {} violates generic ABI-v5 invariants",
                entry.symbol
            ));
        }
        unique(
            entry.params.iter().map(|param| param.name.as_str()),
            "assembly parameter",
        )?;
        if entry
            .preserved
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            != target_names
            || entry
                .locations
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>()
                != target_names
        {
            return Err(format!(
                "assembly {} target mappings are incomplete",
                entry.symbol
            ));
        }
        for target in &manifest.targets {
            let locations = &entry.locations[&target.triple];
            if locations.len() != entry.params.len()
                || locations.iter().any(String::is_empty)
                || entry.preserved[&target.triple].iter().any(String::is_empty)
            {
                return Err(format!(
                    "assembly {} has an invalid {} mapping",
                    entry.symbol, target.triple
                ));
            }
        }
    }
    if manifest.audit.forbidden_symbol_families.is_empty() {
        return Err("audit policy must define forbidden provenance families".into());
    }
    unique(
        manifest
            .audit
            .forbidden_symbol_families
            .iter()
            .map(String::as_str),
        "forbidden provenance family",
    )?;
    Ok(())
}

pub fn generate_v5_artifacts(manifest: &RuntimeManifestV5) -> Result<GeneratedV5Artifacts, String> {
    validate(manifest)?;
    let manifest = canonicalized(manifest);
    Ok(GeneratedV5Artifacts {
        rust: render_rust(&manifest),
        c_header: render_c_header(&manifest),
        gnu_asm: manifest
            .targets
            .iter()
            .filter(|target| target.object_format != "coff")
            .map(|target| {
                (
                    target.triple.clone(),
                    render_asm_target(&manifest, target, false),
                )
            })
            .collect(),
        masm: manifest
            .targets
            .iter()
            .filter(|target| target.object_format == "coff")
            .map(|target| {
                (
                    target.triple.clone(),
                    render_asm_target(&manifest, target, true),
                )
            })
            .collect(),
        abi_json: canonical_json(&manifest)?,
        audit_json: canonical_json(&manifest.audit)?,
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
        fs::write(
            include.join(format!(
                "beskid_runtime_abi_v5_{}.inc",
                target.replace('-', "_")
            )),
            contents,
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn canonical_json(value: &impl Serialize) -> Result<String, String> {
    let mut output = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    output.push('\n');
    Ok(output)
}

fn render_rust(manifest: &RuntimeManifestV5) -> String {
    let json = serde_json::to_string(manifest).expect("serializable manifest");
    let mut registers = manifest
        .assembly
        .iter()
        .flat_map(|entry| entry.preserved.values().flatten())
        .collect::<BTreeSet<_>>();
    let register_rows = registers
        .iter()
        .map(|register| {
            format!(
                "    ({register:?}, crate::abi_v5::AssemblyRegister::{}),\n",
                rust_register_variant(register)
            )
        })
        .collect::<String>();
    registers.clear();
    let assembly_rows = manifest
        .assembly
        .iter()
        .map(|entry| {
            format!(
                "    ({:?}, crate::abi_v5::AssemblySymbol::{}),\n",
                entry.symbol,
                rust_assembly_symbol_variant(&entry.symbol)
            )
        })
        .collect::<String>();
    let trap_rows = manifest
        .traps
        .iter()
        .map(|trap| format!("    ({:?}, {}),\n", trap.name, trap.code))
        .collect::<String>();
    format!(
        "// @generated from runtime_manifest.bsol; do not edit.\n\
pub const ABI_V5_SOURCE_JSON: &str = r#\"{json}\"#;\n\
pub const ABI_V5_RUNTIME_PUBLISHER: &str = {:?};\n\
pub const ABI_V5_RUNTIME_PACKAGE: &str = {:?};\n\
pub const ABI_V5_TRAP_EXIT_STATUS: u32 = {};\n\
pub const ABI_V5_TRAP_DIAGNOSTIC: &str = {:?};\n\
pub const ABI_V5_TYPES: &[(&str, crate::abi_v5::AbiType)] = &[\n\
    (\"void\", crate::abi_v5::AbiType::Void),\n\
    (\"never\", crate::abi_v5::AbiType::Void),\n\
    (\"pointer\", crate::abi_v5::AbiType::Pointer),\n\
    (\"usize\", crate::abi_v5::AbiType::USize),\n\
    (\"isize\", crate::abi_v5::AbiType::ISize),\n\
    (\"i8\", crate::abi_v5::AbiType::I8),\n\
    (\"u8\", crate::abi_v5::AbiType::U8),\n\
    (\"i16\", crate::abi_v5::AbiType::I16),\n\
    (\"u16\", crate::abi_v5::AbiType::U16),\n\
    (\"i32\", crate::abi_v5::AbiType::I32),\n\
    (\"u32\", crate::abi_v5::AbiType::U32),\n\
    (\"i64\", crate::abi_v5::AbiType::I64),\n\
    (\"u64\", crate::abi_v5::AbiType::U64),\n\
    (\"v128\", crate::abi_v5::AbiType::V128),\n\
    (\"f32\", crate::abi_v5::AbiType::F32),\n\
    (\"f64\", crate::abi_v5::AbiType::F64),\n\
];\n\
pub const ABI_V5_ASSEMBLY_REGISTERS: &[(&str, crate::abi_v5::AssemblyRegister)] = &[\n{register_rows}];\n\
pub const ABI_V5_ASSEMBLY_SYMBOLS: &[(&str, crate::abi_v5::AssemblySymbol)] = &[\n{assembly_rows}];\n\
pub const ABI_V5_TRAPS: &[(&str, u8)] = &[\n{trap_rows}];\n",
        manifest.meta.runtime_publisher,
        manifest.meta.runtime_package,
        manifest.meta.trap_exit_status,
        manifest.meta.trap_diagnostic
    )
}

fn rust_register_variant(register: &str) -> String {
    if let Some(number) = register.strip_prefix("xmm") {
        return format!("X86_64Xmm{number}");
    }
    if let Some(number) = register.strip_prefix('x') {
        return format!("Aarch64X{number}");
    }
    if let Some(number) = register.strip_prefix('v') {
        return format!("Aarch64V{number}");
    }
    format!("X86_64{}", pascal_case(register))
}

fn rust_assembly_symbol_variant(symbol: &str) -> String {
    pascal_case(
        symbol
            .strip_prefix("beskid_arch_v5_")
            .expect("validated assembly symbol prefix"),
    )
}

fn pascal_case(value: &str) -> String {
    value
        .split('_')
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| first.to_ascii_uppercase().to_string() + chars.as_str())
                .unwrap_or_default()
        })
        .collect()
}

fn render_c_header(manifest: &RuntimeManifestV5) -> String {
    let mut out = String::from(
        "/* @generated from runtime_manifest.bsol; do not edit. */\n#ifndef BESKID_RUNTIME_ABI_V5_H\n#define BESKID_RUNTIME_ABI_V5_H\n#include <stddef.h>\n#include <stdint.h>\n",
    );
    writeln!(
        out,
        "#define BESKID_RUNTIME_ABI_VERSION {}",
        manifest.meta.abi_version
    )
    .unwrap();
    writeln!(
        out,
        "#define BESKID_TRAP_EXIT_STATUS {}",
        manifest.meta.trap_exit_status
    )
    .unwrap();
    writeln!(
        out,
        "#define BESKID_TRAP_DIAGNOSTIC {:?}",
        manifest.meta.trap_diagnostic
    )
    .unwrap();
    for layout in &manifest.layouts {
        let name = macro_name(layout.name.strip_prefix("Beskid").unwrap_or(&layout.name));
        writeln!(out, "#define BESKID_{name}_SIZE {}", layout.size).unwrap();
        writeln!(out, "#define BESKID_{name}_ALIGNMENT {}", layout.alignment).unwrap();
        for field in &layout.fields {
            writeln!(
                out,
                "#define BESKID_{name}_{}_OFFSET {}",
                macro_name(&field.name),
                field.offset
            )
            .unwrap();
        }
    }
    for function in &manifest.exports {
        let noreturn = if function.result == "never" {
            "_Noreturn "
        } else {
            ""
        };
        let params = if function.params.is_empty() {
            "void".into()
        } else {
            function
                .params
                .iter()
                .map(|param| format!("{} {}", c_type(&param.ty), param.name))
                .collect::<Vec<_>>()
                .join(", ")
        };
        writeln!(
            out,
            "{noreturn}{} {}({});",
            c_type(&function.result),
            function.symbol,
            params
        )
        .unwrap();
    }
    for function in &manifest.assembly {
        writeln!(
            out,
            "{} {}({});",
            c_type(&function.result),
            function.symbol,
            function
                .params
                .iter()
                .map(|param| format!("{} {}", c_type(&param.ty), param.name))
                .collect::<Vec<_>>()
                .join(", ")
        )
        .unwrap();
    }
    out.push_str("#endif\n");
    out
}

fn render_asm_target(manifest: &RuntimeManifestV5, target: &TargetV5, masm: bool) -> String {
    let mut out: String = if masm {
        "; @generated from runtime_manifest.bsol; do not edit.\n".into()
    } else {
        "/* @generated from runtime_manifest.bsol; do not edit. */\n".into()
    };
    let separator = if masm { " EQU " } else { " = " };
    writeln!(
        out,
        "BESKID_RUNTIME_ABI_VERSION{separator}{}",
        manifest.meta.abi_version
    )
    .unwrap();
    {
        let target_name = macro_name(&target.triple);
        if masm {
            writeln!(
                out,
                "BESKID_{target_name}_SYMBOL_PREFIX TEXTEQU <{}>",
                target.symbol_prefix
            )
            .unwrap();
        } else {
            writeln!(
                out,
                "/* BESKID_{target_name}_SYMBOL_PREFIX = {:?} */",
                target.symbol_prefix
            )
            .unwrap();
        }
        writeln!(out, "BESKID_{target_name}_STACK_ALIGNMENT{separator}16").unwrap();
        writeln!(
            out,
            "BESKID_{target_name}_SHADOW_SPACE{separator}{}",
            if target.triple == "x86_64-pc-windows-msvc" {
                32
            } else {
                0
            }
        )
        .unwrap();
        for layout in manifest
            .layouts
            .iter()
            .filter(|layout| layout.target.as_deref() == Some(target.triple.as_str()))
        {
            writeln!(
                out,
                "BESKID_{target_name}_CONTEXT_SIZE{separator}{}",
                layout.size
            )
            .unwrap();
            for field in &layout.fields {
                writeln!(
                    out,
                    "BESKID_{target_name}_CONTEXT_{}_OFFSET{separator}{}",
                    macro_name(&field.name),
                    field.offset
                )
                .unwrap();
            }
        }
        for function in &manifest.assembly {
            let function_name = function
                .symbol
                .strip_prefix("beskid_arch_v5_")
                .unwrap_or(&function.symbol);
            for (param, location) in function
                .params
                .iter()
                .zip(&function.locations[&target.triple])
            {
                let location = location.as_str();
                let key = format!(
                    "BESKID_{}_{}_REGISTER",
                    macro_name(function_name),
                    macro_name(&param.name)
                );
                if masm {
                    writeln!(out, "{key} TEXTEQU <{location}>").unwrap();
                } else {
                    writeln!(out, "#define {key} {location}").unwrap();
                }
            }
            if masm {
                writeln!(
                    out,
                    "; {} preserved: {}",
                    function.symbol,
                    function.preserved[&target.triple].join(",")
                )
                .unwrap();
            } else {
                writeln!(
                    out,
                    "/* {} preserved: {} */",
                    function.symbol,
                    function.preserved[&target.triple].join(",")
                )
                .unwrap();
            }
        }
    }
    out
}

fn c_type(ty: &str) -> &'static str {
    match ty {
        "void" | "never" => "void",
        "pointer" => "void *",
        "usize" => "size_t",
        "isize" => "ptrdiff_t",
        "u8" => "uint8_t",
        "i32" => "int32_t",
        "u32" => "uint32_t",
        "u64" => "uint64_t",
        _ => "uintptr_t",
    }
}
fn macro_name(value: &str) -> String {
    let mut output = String::new();
    for (index, ch) in value.chars().enumerate() {
        if index > 0 && ch.is_ascii_uppercase() && !output.ends_with('_') {
            output.push('_');
        }
        output.push(if ch.is_ascii_alphanumeric() {
            ch.to_ascii_uppercase()
        } else {
            '_'
        });
    }
    output
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

fn canonicalized(manifest: &RuntimeManifestV5) -> RuntimeManifestV5 {
    let mut value = manifest.clone();
    value.targets.sort_by(|a, b| a.triple.cmp(&b.triple));
    value.exports.sort_by(|a, b| a.symbol.cmp(&b.symbol));
    value.intrinsics.sort_by(|a, b| a.name.cmp(&b.name));
    value
        .layouts
        .sort_by(|a, b| a.target.cmp(&b.target).then_with(|| a.name.cmp(&b.name)));
    for layout in &mut value.layouts {
        layout.fields.sort_by_key(|field| field.offset);
    }
    value.platform_imports.sort_by(|a, b| {
        a.target
            .cmp(&b.target)
            .then_with(|| a.symbol.cmp(&b.symbol))
    });
    value.assembly.sort_by(|a, b| a.symbol.cmp(&b.symbol));
    value.traps.sort_by_key(|trap| trap.code);
    value.audit.forbidden_symbol_families.sort();
    value
}

fn one<'a>(blocks: &'a [BsolBlock], kind: &str) -> Result<&'a BsolBlock, String> {
    let mut found = blocks.iter().filter(|block| block.kind == kind);
    let first = found
        .next()
        .ok_or_else(|| format!("missing `{kind}` block"))?;
    if found.next().is_some() {
        Err(format!("duplicate `{kind}` block"))
    } else {
        Ok(first)
    }
}
fn blocks<'a>(blocks: &'a [BsolBlock], kind: &'a str) -> impl Iterator<Item = &'a BsolBlock> {
    blocks.iter().filter(move |block| block.kind == kind)
}
fn label(block: &BsolBlock) -> Result<String, String> {
    block
        .label
        .as_ref()
        .map(|label| label.value.clone())
        .ok_or_else(|| format!("`{}` requires a label", block.kind))
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
                    return Err(format!(
                        "duplicate field `{}` in `{}`",
                        entry.key, block.kind
                    ));
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
    string_field(block, key)?
        .parse()
        .map_err(|_| format!("`{key}` must be u32"))
}
fn u64_field(block: &BsolBlock, key: &str) -> Result<u64, String> {
    string_field(block, key)?
        .parse()
        .map_err(|_| format!("`{key}` must be u64"))
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
                Ok(ParameterV5 {
                    name: map_string(map, "name")?,
                    ty: map_string(map, "type")?,
                })
            }
            _ => Err(format!("`{key}` entries must be inline maps")),
        })
        .collect()
}
fn fields(block: &BsolBlock) -> Result<Vec<FieldV5>, String> {
    list_items(block, "fields")?
        .iter()
        .map(|item| match item {
            BsolListItem::InlineMap(map) => {
                ensure_map_fields(map, &["name", "offset", "type"])?;
                Ok(FieldV5 {
                    name: map_string(map, "name")?,
                    offset: map_string(map, "offset")?
                        .parse()
                        .map_err(|_| "field offset must be u64")?,
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
fn parse_assembly(block: &BsolBlock) -> Result<AssemblyV5, String> {
    let mut allowed = vec!["params".to_string(), "returns".to_string()];
    for target in [
        "x86_64-unknown-linux-gnu",
        "aarch64-apple-darwin",
        "x86_64-pc-windows-msvc",
    ] {
        let slug = target.replace('-', "_");
        allowed.push(format!("{slug}_preserved"));
        allowed.push(format!("{slug}_locations"));
    }
    ensure_fields(
        block,
        &allowed.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    let mut preserved = BTreeMap::new();
    let mut locations = BTreeMap::new();
    for target in [
        "x86_64-unknown-linux-gnu",
        "aarch64-apple-darwin",
        "x86_64-pc-windows-msvc",
    ] {
        let slug = target.replace('-', "_");
        preserved.insert(
            target.into(),
            list_field(block, &format!("{slug}_preserved"))?,
        );
        locations.insert(
            target.into(),
            list_field(block, &format!("{slug}_locations"))?,
        );
    }
    Ok(AssemblyV5 {
        symbol: label(block)?,
        params: parameters(block, "params")?,
        result: string_field(block, "returns")?,
        preserved,
        locations,
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
