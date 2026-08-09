use std::collections::BTreeMap;
use std::fmt::Write as _;

use super::model::{RuntimeManifestV5, TargetV5};

pub(super) fn render_rust(
    manifest: &RuntimeManifestV5,
    gnu_asm: &BTreeMap<String, String>,
    masm: &BTreeMap<String, String>,
) -> String {
    let json = serde_json::to_string(manifest).expect("serializable manifest");
    let trap_rows =
        manifest.traps.iter().map(|trap| format!("    ({:?}, {}),\n", trap.name, trap.code)).collect::<String>();
    let target_rows = manifest
        .targets
        .iter()
        .map(|target| {
            format!(
                "    GeneratedTarget {{ triple: {:?}, endianness: {:?}, pointer_width: {}, calling_convention: {:?}, object_format: {:?}, symbol_prefix: {:?}, stack_alignment: {}, shadow_space: {} }},\n",
                target.triple,
                target.endianness,
                target.pointer_width,
                target.calling_convention,
                target.object_format,
                target.symbol_prefix,
                target.stack_alignment,
                target.shadow_space,
            )
        })
        .collect::<String>();
    let asm_rows = gnu_asm
        .iter()
        .chain(masm)
        .map(|(target, source)| format!("    ({target:?}, {source:?}),\n"))
        .collect::<String>();
    let soft_builtin_rows = manifest
        .soft_builtins
        .iter()
        .map(|builtin| {
            let params = builtin
                .params
                .iter()
                .map(|parameter| soft_builtin_param_kind(&parameter.ty))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "    crate::BuiltinFnSpec {{ symbol: {:?}, params: &[{}], returns: {} }},\n",
                builtin.symbol,
                params,
                soft_builtin_return_kind(&builtin.result),
            )
        })
        .collect::<String>();
    let corelib_service_binding_rows = manifest
        .corelib_services
        .iter()
        .flat_map(|service| {
            service.target_bindings.iter().map(|binding| {
                let params = service.params.iter().map(|param| format!("{:?}", param.ty)).collect::<Vec<_>>().join(", ");
                let os_imports = binding.os_imports.iter().map(|import| format!("{import:?}")).collect::<Vec<_>>().join(", ");
                format!(
                    "    GeneratedCorelibServiceBinding {{ service: {:?}, adapter: {:?}, params: &[{}], result: {:?}, target: {:?}, implementation: {:?}, os_imports: &[{}] }},\n",
                    service.name,
                    service.adapter,
                    params,
                    service.result,
                    binding.target,
                    binding.implementation,
                    os_imports,
                )
            })
        })
        .collect::<String>();
    let core_args_entry_adapter_rows = manifest.entry_adapters.iter().map(|adapter| {
        let os_imports = adapter.os_imports.iter().map(|import| format!("{import:?}")).collect::<Vec<_>>().join(", ");
        format!("    GeneratedCoreArgsEntryAdapter {{ target: {:?}, executable_entry: {:?}, program_entry: {:?}, capture: {:?}, handoff: {:?}, ownership: {:?}, entry_source: {:?}, os_imports: &[{}] }},\n", adapter.target, adapter.executable_entry, adapter.program_entry, adapter.capture, adapter.handoff, adapter.ownership, adapter.entry_source, os_imports)
    }).collect::<String>();
    format!(
        "// @generated from runtime_manifest.bsol; do not edit.\n\
pub const ABI_V5_SOURCE_JSON: &str = r#\"{json}\"#;\n\
pub const ABI_V5_RUNTIME_PUBLISHER: &str = {:?};\n\
pub const ABI_V5_RUNTIME_PACKAGE: &str = {:?};\n\
pub const ABI_V5_TRAP_EXIT_STATUS: u32 = {};\n\
pub const ABI_V5_TRAP_DIAGNOSTIC: &str = {:?};\n\
#[derive(Debug, Clone, Copy)]\n\
pub struct GeneratedTarget {{\n\
    pub triple: &'static str,\n\
    pub endianness: &'static str,\n\
    pub pointer_width: u8,\n\
    pub calling_convention: &'static str,\n\
    pub object_format: &'static str,\n\
    pub symbol_prefix: &'static str,\n\
    pub stack_alignment: u32,\n\
    pub shadow_space: u32,\n\
}}\n\
pub const ABI_V5_TARGETS: &[GeneratedTarget] = &[\n{target_rows}];\n\
pub const ABI_V5_ASM_INCLUDES: &[(&str, &str)] = &[\n{asm_rows}];\n\
/// Process-linked soft builtins declared by the ABI-v5 source manifest.\n\
pub const ABI_V5_SOFT_BUILTINS: &[crate::BuiltinFnSpec] = &[\n{soft_builtin_rows}];\n\
#[derive(Debug, Clone, Copy)]\n\
pub struct GeneratedCorelibServiceBinding {{\n\
    pub service: &'static str,\n\
    pub adapter: &'static str,\n\
    pub params: &'static [&'static str],\n\
    pub result: &'static str,\n\
    pub target: &'static str,\n\
    pub implementation: &'static str,\n\
    pub os_imports: &'static [&'static str],\n\
}}\n\
pub const ABI_V5_CORELIB_SERVICE_BINDINGS: &[GeneratedCorelibServiceBinding] = &[\n{corelib_service_binding_rows}];\n\
#[derive(Debug, Clone, Copy)]\n\
pub struct GeneratedCoreArgsEntryAdapter {{\n\
    pub target: &'static str,\n\
    pub executable_entry: &'static str,\n\
    pub program_entry: &'static str,\n\
    pub capture: &'static str,\n\
    pub handoff: &'static str,\n\
    pub ownership: &'static str,\n\
    pub entry_source: &'static str,\n\
    pub os_imports: &'static [&'static str],\n\
}}\n\
pub const ABI_V5_CORE_ARGS_ENTRY_ADAPTERS: &[GeneratedCoreArgsEntryAdapter] = &[\n{core_args_entry_adapter_rows}];\n\
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
pub const ABI_V5_TRAPS: &[(&str, u8)] = &[\n{trap_rows}];\n",
        manifest.meta.runtime_publisher,
        manifest.meta.runtime_package,
        manifest.meta.trap_exit_status,
        manifest.meta.trap_diagnostic
    )
}

fn soft_builtin_param_kind(ty: &str) -> &'static str {
    match ty {
        "pointer" | "string" => "crate::AbiParamKind::Ptr",
        "usize" | "isize" | "i8" | "u8" | "i16" | "u16" | "i32" | "u32" | "i64" | "u64" | "f32" | "f64" => {
            "crate::AbiParamKind::I64"
        }
        other => panic!("invalid soft builtin parameter type `{other}`"),
    }
}

fn soft_builtin_return_kind(ty: &str) -> &'static str {
    match ty {
        "void" => "crate::AbiReturnKind::Void",
        "never" => "crate::AbiReturnKind::Never",
        "pointer" | "string" => "crate::AbiReturnKind::Ptr",
        "i32" | "u32" | "i8" | "u8" | "i16" | "u16" => "crate::AbiReturnKind::I32",
        "usize" | "isize" | "i64" | "u64" | "f32" | "f64" => "crate::AbiReturnKind::I64",
        other => panic!("invalid soft builtin result type `{other}`"),
    }
}

pub(super) fn render_c_header(manifest: &RuntimeManifestV5) -> String {
    let mut out = String::from(
        "/* @generated from runtime_manifest.bsol; do not edit. */\n#ifndef BESKID_RUNTIME_ABI_V5_H\n#define BESKID_RUNTIME_ABI_V5_H\n#include <stddef.h>\n#include <stdint.h>\n",
    );
    writeln!(out, "#define BESKID_RUNTIME_ABI_VERSION {}", manifest.meta.abi_version).unwrap();
    writeln!(out, "#define BESKID_TRAP_EXIT_STATUS {}", manifest.meta.trap_exit_status).unwrap();
    writeln!(out, "#define BESKID_TRAP_DIAGNOSTIC {:?}", manifest.meta.trap_diagnostic).unwrap();
    if manifest.corelib_services.iter().any(|service| service.result == "string") {
        out.push_str("struct BeskidStr;\n");
    }
    for layout in &manifest.layouts {
        let name = macro_name(layout.name.strip_prefix("Beskid").unwrap_or(&layout.name));
        writeln!(out, "#define BESKID_{name}_SIZE {}", layout.size).unwrap();
        writeln!(out, "#define BESKID_{name}_ALIGNMENT {}", layout.alignment).unwrap();
        for field in &layout.fields {
            writeln!(out, "#define BESKID_{name}_{}_OFFSET {}", macro_name(&field.name), field.offset).unwrap();
        }
    }
    for function in &manifest.exports {
        let noreturn = if function.result == "never" { "_Noreturn " } else { "" };
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
        writeln!(out, "{noreturn}{} {}({});", c_type(&function.result), function.symbol, params).unwrap();
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
    for service in &manifest.corelib_services {
        let params = if service.params.is_empty() {
            "void".into()
        } else {
            service
                .params
                .iter()
                .map(|param| format!("{} {}", corelib_service_c_type(&param.ty), param.name))
                .collect::<Vec<_>>()
                .join(", ")
        };
        writeln!(out, "{} {}({});", corelib_service_c_type(&service.result), service.adapter, params).unwrap();
    }
    out.push_str("#endif\n");
    out
}

pub(super) fn render_asm_target(manifest: &RuntimeManifestV5, target: &TargetV5, masm: bool) -> String {
    let mut out: String = if masm {
        "; @generated from runtime_manifest.bsol; do not edit.\n".into()
    } else {
        "/* @generated from runtime_manifest.bsol; do not edit. */\n".into()
    };
    let separator = if masm { " EQU " } else { " = " };
    writeln!(out, "BESKID_RUNTIME_ABI_VERSION{separator}{}", manifest.meta.abi_version).unwrap();
    {
        let target_name = macro_name(&target.triple);
        if masm {
            writeln!(out, "BESKID_{target_name}_SYMBOL_PREFIX TEXTEQU <{}>", target.symbol_prefix).unwrap();
        } else {
            writeln!(out, "/* BESKID_{target_name}_SYMBOL_PREFIX = {:?} */", target.symbol_prefix).unwrap();
        }
        writeln!(out, "BESKID_{target_name}_STACK_ALIGNMENT{separator}{}", target.stack_alignment).unwrap();
        writeln!(out, "BESKID_{target_name}_SHADOW_SPACE{separator}{}", target.shadow_space).unwrap();
        for layout in manifest.layouts.iter().filter(|layout| layout.target.as_deref() == Some(target.triple.as_str()))
        {
            writeln!(out, "BESKID_{target_name}_CONTEXT_SIZE{separator}{}", layout.size).unwrap();
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
            let function_name = function.symbol.strip_prefix("beskid_arch_v5_").unwrap_or(&function.symbol);
            for (param, location) in function.params.iter().zip(&function.locations[&target.triple]) {
                let (kind, operand) = match location {
                    ParameterLocationV5::Register { register } => ("REGISTER", register.clone()),
                    ParameterLocationV5::Stack { base, offset } => {
                        let operand = if masm { format!("[{base} + {offset}]") } else { format!("{offset}(%{base})") };
                        ("STACK_OPERAND", operand)
                    }
                };
                let key = format!("BESKID_{}_{}_{}", macro_name(function_name), macro_name(&param.name), kind,);
                if masm {
                    writeln!(out, "{key} TEXTEQU <{operand}>").unwrap();
                } else {
                    writeln!(out, "#define {key} {operand}").unwrap();
                }
            }
            if masm {
                writeln!(out, "; {} preserved: {}", function.symbol, function.preserved[&target.triple].join(","))
                    .unwrap();
            } else {
                writeln!(out, "/* {} preserved: {} */", function.symbol, function.preserved[&target.triple].join(","))
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
        "i64" => "int64_t",
        "u64" => "uint64_t",
        _ => "uintptr_t",
    }
}

fn corelib_service_c_type(ty: &str) -> &'static str {
    match ty {
        "string" => "struct BeskidStr *",
        _ => c_type(ty),
    }
}
fn macro_name(value: &str) -> String {
    let mut output = String::new();
    for (index, ch) in value.chars().enumerate() {
        if index > 0 && ch.is_ascii_uppercase() && !output.ends_with('_') {
            output.push('_');
        }
        output.push(if ch.is_ascii_alphanumeric() { ch.to_ascii_uppercase() } else { '_' });
    }
    output
}
