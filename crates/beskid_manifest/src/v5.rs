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
    pub gnu_asm: String,
    pub masm: String,
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
    let expected_targets = [
        "aarch64-apple-darwin",
        "x86_64-pc-windows-msvc",
        "x86_64-unknown-linux-gnu",
    ];
    let actual = manifest
        .targets
        .iter()
        .map(|target| target.triple.as_str())
        .collect::<BTreeSet<_>>();
    if actual != expected_targets.into_iter().collect() {
        return Err("manifest must define exactly the three ABI-v5 targets".into());
    }
    for target in &manifest.targets {
        let expected = match target.triple.as_str() {
            "x86_64-unknown-linux-gnu" => ("little", 64, "system_v", "elf", ""),
            "aarch64-apple-darwin" => ("little", 64, "apple_aarch64", "macho", "_"),
            "x86_64-pc-windows-msvc" => ("little", 64, "windows_x64", "coff", ""),
            _ => unreachable!(),
        };
        if (
            target.endianness.as_str(),
            target.pointer_width,
            target.calling_convention.as_str(),
            target.object_format.as_str(),
            target.symbol_prefix.as_str(),
        ) != expected
        {
            return Err(format!(
                "target `{}` properties do not match the ABI-v5 contract",
                target.triple
            ));
        }
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
    let expected_exports = [
        "beskid_library_attach_v5",
        "beskid_library_detach_v5",
        "beskid_rt_v5_abi_version",
        "beskid_rt_v5_process_init",
        "beskid_rt_v5_process_shutdown",
        "beskid_rt_v5_thread_attach",
        "beskid_rt_v5_thread_detach",
        "beskid_rt_v5_trap",
    ];
    if manifest
        .exports
        .iter()
        .map(|entry| entry.symbol.as_str())
        .collect::<BTreeSet<_>>()
        != expected_exports.into_iter().collect()
    {
        return Err("runtime export set is not exact".into());
    }
    let expected_intrinsics = [
        "memory_compare",
        "memory_copy",
        "memory_set",
        "native_word_from_pointer",
        "pointer_add",
        "pointer_from_native_word",
        "raw_byte_load",
        "raw_byte_store",
        "raw_word_load",
        "raw_word_store",
        "system_allocate",
        "system_free",
        "tls_get",
        "tls_set",
        "trap",
    ];
    if manifest
        .intrinsics
        .iter()
        .map(|entry| entry.name.as_str())
        .collect::<BTreeSet<_>>()
        != expected_intrinsics.into_iter().collect()
    {
        return Err("runtime intrinsic set is not exact".into());
    }
    for entry in &manifest.exports {
        let expected = match entry.symbol.as_str() {
            "beskid_rt_v5_abi_version" => "->u32",
            "beskid_library_attach_v5" => "pointer->i32",
            "beskid_library_detach_v5"
            | "beskid_rt_v5_process_shutdown"
            | "beskid_rt_v5_thread_detach" => "pointer->void",
            "beskid_rt_v5_process_init" | "beskid_rt_v5_thread_attach" => "pointer->pointer",
            "beskid_rt_v5_trap" => "u8,pointer,usize->never",
            _ => unreachable!(),
        };
        if signature(&entry.params, &entry.result) != expected {
            return Err(format!("export `{}` signature is not exact", entry.symbol));
        }
    }
    for entry in &manifest.intrinsics {
        let expected = match entry.name.as_str() {
            "native_word_from_pointer" => "pointer->usize",
            "pointer_from_native_word" => "usize->pointer",
            "pointer_add" => "pointer,usize->pointer",
            "raw_word_load" => "pointer->usize",
            "raw_word_store" => "pointer,usize->void",
            "raw_byte_load" => "pointer->u8",
            "raw_byte_store" => "pointer,u8->void",
            "memory_set" => "pointer,u8,usize->void",
            "memory_copy" => "pointer,pointer,usize->void",
            "memory_compare" => "pointer,pointer,usize->i32",
            "system_allocate" => "usize,usize->pointer",
            "system_free" => "pointer,usize->void",
            "tls_get" => "->pointer",
            "tls_set" => "pointer->void",
            "trap" => "u8,pointer,usize->never",
            _ => unreachable!(),
        };
        if signature(&entry.params, &entry.result) != expected
            || entry.capability != format!("runtime.bootstrap.{}", entry.name)
        {
            return Err(format!("intrinsic `{}` contract is not exact", entry.name));
        }
    }
    unique(
        manifest
            .platform_imports
            .iter()
            .map(|entry| (entry.target.as_str(), entry.symbol.as_str())),
        "platform import",
    )?;
    if manifest.platform_imports.len() != 13 {
        return Err("platform import set is not exact".into());
    }
    for entry in &manifest.platform_imports {
        let (library, expected) = match (entry.target.as_str(), entry.symbol.as_str()) {
            ("x86_64-unknown-linux-gnu", "_exit") | ("aarch64-apple-darwin", "_exit") => (
                if entry.target.starts_with("aarch64") {
                    "libSystem"
                } else {
                    "libc"
                },
                "i32->never",
            ),
            ("x86_64-unknown-linux-gnu", "mmap") | ("aarch64-apple-darwin", "mmap") => (
                if entry.target.starts_with("aarch64") {
                    "libSystem"
                } else {
                    "libc"
                },
                "pointer,usize,i32,i32,i32,i64->pointer",
            ),
            ("x86_64-unknown-linux-gnu", "munmap") | ("aarch64-apple-darwin", "munmap") => (
                if entry.target.starts_with("aarch64") {
                    "libSystem"
                } else {
                    "libc"
                },
                "pointer,usize->i32",
            ),
            ("x86_64-unknown-linux-gnu", "write") | ("aarch64-apple-darwin", "write") => (
                if entry.target.starts_with("aarch64") {
                    "libSystem"
                } else {
                    "libc"
                },
                "i32,pointer,usize->isize",
            ),
            ("x86_64-pc-windows-msvc", "ExitProcess") => ("kernel32", "u32->never"),
            ("x86_64-pc-windows-msvc", "GetStdHandle") => ("kernel32", "i32->pointer"),
            ("x86_64-pc-windows-msvc", "VirtualAlloc") => {
                ("kernel32", "pointer,usize,u32,u32->pointer")
            }
            ("x86_64-pc-windows-msvc", "VirtualFree") => ("kernel32", "pointer,usize,u32->i32"),
            ("x86_64-pc-windows-msvc", "WriteFile") => {
                ("kernel32", "pointer,pointer,u32,pointer,pointer->i32")
            }
            _ => {
                return Err(format!(
                    "unknown platform import `{}` for `{}`",
                    entry.symbol, entry.target
                ));
            }
        };
        if entry.library != library || signature(&entry.params, &entry.result) != expected {
            return Err(format!(
                "platform import `{}` contract is not exact",
                entry.symbol
            ));
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
    if manifest.assembly.len() != 2 {
        return Err("exactly two assembly functions are permitted".into());
    }
    for entry in &manifest.assembly {
        let expected_names: &[&str] = match entry.symbol.as_str() {
            "beskid_arch_v5_context_init" => &[
                "context",
                "stack_top",
                "entry",
                "argument",
                "return_trampoline",
            ],
            "beskid_arch_v5_context_switch" => &["from", "to"],
            _ => return Err("only the two approved assembly symbols are permitted".into()),
        };
        if entry
            .params
            .iter()
            .map(|param| param.name.as_str())
            .collect::<Vec<_>>()
            != expected_names
        {
            return Err(format!(
                "assembly `{}` has an invalid named parameter contract",
                entry.symbol
            ));
        }
        for target in &manifest.targets {
            if !entry.preserved.contains_key(&target.triple)
                || entry.locations.get(&target.triple).map(Vec::len) != Some(entry.params.len())
            {
                return Err(format!(
                    "assembly `{}` lacks exact `{}` preserved/location mapping",
                    entry.symbol, target.triple
                ));
            }
            let expected_preserved: &[&str] = match target.triple.as_str() {
                "x86_64-unknown-linux-gnu" => &["rbx", "rbp", "r12", "r13", "r14", "r15"],
                "aarch64-apple-darwin" => &[
                    "x19", "x20", "x21", "x22", "x23", "x24", "x25", "x26", "x27", "x28", "x29",
                    "v8", "v9", "v10", "v11", "v12", "v13", "v14", "v15",
                ],
                "x86_64-pc-windows-msvc" => &[
                    "rbx", "rbp", "rdi", "rsi", "r12", "r13", "r14", "r15", "xmm6", "xmm7", "xmm8",
                    "xmm9", "xmm10", "xmm11", "xmm12", "xmm13", "xmm14", "xmm15",
                ],
                _ => unreachable!(),
            };
            let expected_locations: &[&str] = match (target.triple.as_str(), entry.symbol.as_str())
            {
                ("x86_64-unknown-linux-gnu", "beskid_arch_v5_context_init") => {
                    &["rdi", "rsi", "rdx", "rcx", "r8"]
                }
                ("x86_64-unknown-linux-gnu", _) => &["rdi", "rsi"],
                ("aarch64-apple-darwin", "beskid_arch_v5_context_init") => {
                    &["x0", "x1", "x2", "x3", "x4"]
                }
                ("aarch64-apple-darwin", _) => &["x0", "x1"],
                ("x86_64-pc-windows-msvc", "beskid_arch_v5_context_init") => {
                    &["rcx", "rdx", "r8", "r9", "stack+40"]
                }
                ("x86_64-pc-windows-msvc", _) => &["rcx", "rdx"],
                _ => unreachable!(),
            };
            if entry.preserved[&target.triple]
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                != expected_preserved
                || entry.locations[&target.triple]
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    != expected_locations
            {
                return Err(format!(
                    "assembly `{}` has invalid `{}` ABI mapping",
                    entry.symbol, target.triple
                ));
            }
        }
    }
    let required_forbidden = [
        "rust",
        "_rust",
        "__rust",
        "core::panicking",
        "std::panicking",
        "alloc::alloc",
        "panic",
        "_Unwind",
        "__Unwind",
        "eh_personality",
        "gcc_personality",
        "abfall",
        "corosensei",
    ];
    if required_forbidden.iter().any(|family| {
        !manifest
            .audit
            .forbidden_symbol_families
            .iter()
            .any(|actual| actual == family)
    }) {
        return Err("audit policy omits a forbidden Rust/runtime provenance family".into());
    }
    Ok(())
}

pub fn generate_v5_artifacts(manifest: &RuntimeManifestV5) -> Result<GeneratedV5Artifacts, String> {
    validate(manifest)?;
    let manifest = canonicalized(manifest);
    Ok(GeneratedV5Artifacts {
        rust: render_rust(&manifest),
        c_header: render_c_header(&manifest),
        gnu_asm: render_asm(&manifest, false),
        masm: render_asm(&manifest, true),
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
        (include.join("beskid_runtime_abi_v5.inc"), artifacts.gnu_asm),
        (
            include.join("beskid_runtime_abi_v5_masm.inc"),
            artifacts.masm,
        ),
        (include.join("abi-v5.json"), artifacts.abi_json),
        (include.join("abi-v5-audit.json"), artifacts.audit_json),
    ] {
        fs::write(path, contents).map_err(|error| error.to_string())?;
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
    format!(
        "// @generated from runtime_manifest.bsol; do not edit.\n\
pub const ABI_V5_SOURCE_JSON: &str = r#\"{json}\"#;\n\
pub const ABI_V5_RUNTIME_PUBLISHER: &str = {:?};\n\
pub const ABI_V5_RUNTIME_PACKAGE: &str = {:?};\n\
pub const ABI_V5_TRAP_EXIT_STATUS: u32 = {};\n\
pub const ABI_V5_TRAP_DIAGNOSTIC: &str = {:?};\n",
        manifest.meta.runtime_publisher,
        manifest.meta.runtime_package,
        manifest.meta.trap_exit_status,
        manifest.meta.trap_diagnostic
    )
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
        writeln!(
            out,
            "{noreturn}{} {}({});",
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

fn render_asm(manifest: &RuntimeManifestV5, masm: bool) -> String {
    let mut out: String = if masm {
        "; @generated from runtime_manifest.bsol; do not edit.\n".into()
    } else {
        "# @generated from runtime_manifest.bsol; do not edit.\n".into()
    };
    let separator = if masm { " EQU " } else { " = " };
    let comment = if masm { ";" } else { "#" };
    writeln!(
        out,
        "BESKID_RUNTIME_ABI_VERSION{separator}{}",
        manifest.meta.abi_version
    )
    .unwrap();
    for target in &manifest.targets {
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
                "{comment} BESKID_{target_name}_SYMBOL_PREFIX = {:?}",
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
                    "BESKID_{target_name}_{}_{}_REGISTER",
                    macro_name(function_name),
                    macro_name(&param.name)
                );
                if masm {
                    writeln!(out, "{key} TEXTEQU <{location}>").unwrap();
                } else {
                    writeln!(out, "{key}{separator}{location}").unwrap();
                }
            }
            writeln!(
                out,
                "{comment} {} preserved: {}",
                function.symbol,
                function.preserved[&target.triple].join(",")
            )
            .unwrap();
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
fn signature(params: &[ParameterV5], result: &str) -> String {
    format!(
        "{}->{result}",
        params
            .iter()
            .map(|param| param.ty.as_str())
            .collect::<Vec<_>>()
            .join(",")
    )
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
    match value(block, key) {
        Ok(value) => string_value(value).map(Some),
        Err(_) => Ok(None),
    }
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
            BsolListItem::InlineMap(map) => Ok(ParameterV5 {
                name: map_string(map, "name")?,
                ty: map_string(map, "type")?,
            }),
            _ => Err(format!("`{key}` entries must be inline maps")),
        })
        .collect()
}
fn fields(block: &BsolBlock) -> Result<Vec<FieldV5>, String> {
    list_items(block, "fields")?
        .iter()
        .map(|item| match item {
            BsolListItem::InlineMap(map) => Ok(FieldV5 {
                name: map_string(map, "name")?,
                offset: map_string(map, "offset")?
                    .parse()
                    .map_err(|_| "field offset must be u64")?,
                ty: map_string(map, "type")?,
            }),
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
