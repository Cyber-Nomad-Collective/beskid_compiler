use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use super::{
    ABI_V5, AbiFieldLayout, AbiFunction, AbiLayout, AbiManifestV5, AbiType, AssemblyExport,
    ManifestValidationError, PlatformImport, RuntimeIntrinsic, TargetMetadata, TargetTriple,
    TrapCode,
};

pub const CANONICAL_RUNTIME_PACKAGE_PUBLISHER: &str = "beskid-lang.org";
pub const CANONICAL_RUNTIME_PACKAGE_NAME: &str = "beskid-runtime-native";
pub const TRAP_EXIT_STATUS: u8 = 101;
pub const TRAP_DIAGNOSTIC_PREFIX: &str = "beskid runtime trap v5";
const STACK_ALIGNMENT: u64 = 16;
const FORBIDDEN_RUST_SYMBOLS: &[&str] = &[
    "__rust_alloc",
    "__rust_dealloc",
    "_Unwind_Resume",
    "abfall",
    "corosensei",
    "panic_unwind",
    "rust_begin_unwind",
    "rust_eh_personality",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimePackageIdentity {
    pub publisher: String,
    pub name: String,
    pub abi_version: u32,
}

pub fn canonical_runtime_package() -> RuntimePackageIdentity {
    RuntimePackageIdentity {
        publisher: CANONICAL_RUNTIME_PACKAGE_PUBLISHER.into(),
        name: CANONICAL_RUNTIME_PACKAGE_NAME.into(),
        abi_version: ABI_V5,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeAuditMetadata {
    pub allowed_imports: Vec<String>,
    pub allowed_exports: Vec<String>,
    pub forbidden_rust_symbols: Vec<String>,
    pub layout_hash: String,
    pub runtime_source_hash: String,
}

impl RuntimeAuditMetadata {
    pub fn for_manifest(
        manifest: &AbiManifestV5,
        runtime_source_hash: &str,
    ) -> Result<Self, ManifestValidationError> {
        manifest.validate()?;
        let mut allowed_imports = manifest
            .platform_imports
            .iter()
            .map(|entry| entry.symbol.clone())
            .chain(
                manifest
                    .assembly_exports
                    .iter()
                    .map(|entry| entry.symbol.as_str().into()),
            )
            .collect::<Vec<_>>();
        allowed_imports.sort();
        Ok(Self {
            allowed_imports,
            allowed_exports: manifest
                .exports
                .iter()
                .map(|entry| entry.symbol.clone())
                .collect(),
            forbidden_rust_symbols: FORBIDDEN_RUST_SYMBOLS
                .iter()
                .map(|symbol| (*symbol).into())
                .collect(),
            layout_hash: manifest.layout_hash(),
            runtime_source_hash: runtime_source_hash.into(),
        })
    }

    pub fn validate(&self, manifest: &AbiManifestV5) -> Result<(), ManifestValidationError> {
        let expected = Self::for_manifest(manifest, &self.runtime_source_hash)?;
        if self == &expected {
            Ok(())
        } else {
            Err(ManifestValidationError::InvalidRuntimeAuditMetadata)
        }
    }
}

impl AbiManifestV5 {
    pub fn canonical_runtime(target: TargetMetadata) -> Self {
        Self {
            abi_version: ABI_V5,
            imports: Vec::new(),
            exports: lifecycle_exports(),
            layouts: canonical_layouts(&target),
            trusted_runtime_package: Some(canonical_runtime_package()),
            trusted_runtime_intrinsics: bootstrap_intrinsics(),
            platform_imports: platform_imports(&target),
            assembly_exports: AssemblyExport::required_for_target(&target),
            traps: TrapCode::ALL.to_vec(),
            target,
        }
    }

    pub(super) fn validate_canonical_bootstrap_contract(
        &self,
    ) -> Result<(), ManifestValidationError> {
        if !self.imports.is_empty() {
            return Err(ManifestValidationError::InvalidRuntimeImportSet {
                actual: self.imports.clone(),
            });
        }
        if self.exports != lifecycle_exports() {
            return Err(ManifestValidationError::InvalidRuntimeExportSet {
                actual: self.exports.clone(),
            });
        }
        if self.trusted_runtime_intrinsics != bootstrap_intrinsics() {
            return Err(ManifestValidationError::InvalidRuntimeIntrinsicSet {
                actual: self.trusted_runtime_intrinsics.clone(),
            });
        }
        if self.platform_imports != platform_imports(&self.target) {
            return Err(ManifestValidationError::InvalidPlatformImportSet {
                actual: self.platform_imports.clone(),
            });
        }
        if self.layouts != canonical_layouts(&self.target) {
            return Err(ManifestValidationError::InvalidRuntimeLayoutSet {
                actual: self.layouts.clone(),
            });
        }
        Ok(())
    }
}

fn function(symbol: &str, params: &[AbiType], result: AbiType) -> AbiFunction {
    AbiFunction {
        symbol: symbol.into(),
        params: params.to_vec(),
        result,
    }
}

fn lifecycle_exports() -> Vec<AbiFunction> {
    vec![
        function("beskid_rt_v5_abi_version", &[], AbiType::U32),
        function(
            "beskid_library_attach_v5",
            &[AbiType::Pointer],
            AbiType::I32,
        ),
        function(
            "beskid_library_detach_v5",
            &[AbiType::Pointer],
            AbiType::Void,
        ),
        function(
            "beskid_rt_v5_process_init",
            &[AbiType::Pointer],
            AbiType::Pointer,
        ),
        function(
            "beskid_rt_v5_process_shutdown",
            &[AbiType::Pointer],
            AbiType::Void,
        ),
        function(
            "beskid_rt_v5_thread_attach",
            &[AbiType::Pointer],
            AbiType::Pointer,
        ),
        function(
            "beskid_rt_v5_thread_detach",
            &[AbiType::Pointer],
            AbiType::Void,
        ),
        function(
            "beskid_rt_v5_trap",
            &[AbiType::U8, AbiType::Pointer, AbiType::USize],
            AbiType::Void,
        ),
    ]
}

fn intrinsic(name: &str, params: &[AbiType], result: AbiType) -> RuntimeIntrinsic {
    RuntimeIntrinsic {
        name: name.into(),
        capability: format!("runtime.bootstrap.{name}"),
        params: params.to_vec(),
        result,
    }
}

fn bootstrap_intrinsics() -> Vec<RuntimeIntrinsic> {
    vec![
        intrinsic(
            "native_word_from_pointer",
            &[AbiType::Pointer],
            AbiType::USize,
        ),
        intrinsic(
            "pointer_from_native_word",
            &[AbiType::USize],
            AbiType::Pointer,
        ),
        intrinsic(
            "pointer_add",
            &[AbiType::Pointer, AbiType::USize],
            AbiType::Pointer,
        ),
        intrinsic("raw_word_load", &[AbiType::Pointer], AbiType::USize),
        intrinsic(
            "raw_word_store",
            &[AbiType::Pointer, AbiType::USize],
            AbiType::Void,
        ),
        intrinsic("raw_byte_load", &[AbiType::Pointer], AbiType::U8),
        intrinsic(
            "raw_byte_store",
            &[AbiType::Pointer, AbiType::U8],
            AbiType::Void,
        ),
        intrinsic(
            "memory_set",
            &[AbiType::Pointer, AbiType::U8, AbiType::USize],
            AbiType::Void,
        ),
        intrinsic(
            "memory_copy",
            &[AbiType::Pointer, AbiType::Pointer, AbiType::USize],
            AbiType::Void,
        ),
        intrinsic(
            "memory_compare",
            &[AbiType::Pointer, AbiType::Pointer, AbiType::USize],
            AbiType::I32,
        ),
        intrinsic(
            "system_allocate",
            &[AbiType::USize, AbiType::USize],
            AbiType::Pointer,
        ),
        intrinsic(
            "system_free",
            &[AbiType::Pointer, AbiType::USize],
            AbiType::Void,
        ),
        intrinsic("tls_get", &[], AbiType::Pointer),
        intrinsic("tls_set", &[AbiType::Pointer], AbiType::Void),
        intrinsic(
            "trap",
            &[AbiType::U8, AbiType::Pointer, AbiType::USize],
            AbiType::Void,
        ),
    ]
}

fn platform_import(
    symbol: &str,
    library: &str,
    params: &[AbiType],
    result: AbiType,
) -> PlatformImport {
    PlatformImport {
        symbol: symbol.into(),
        library: library.into(),
        params: params.to_vec(),
        result,
    }
}

fn platform_imports(target: &TargetMetadata) -> Vec<PlatformImport> {
    match target.triple {
        TargetTriple::X86_64UnknownLinuxGnu => posix_platform_imports("libc"),
        TargetTriple::Aarch64AppleDarwin => posix_platform_imports("libSystem"),
        TargetTriple::X86_64PcWindowsMsvc => vec![
            platform_import("ExitProcess", "kernel32", &[AbiType::U32], AbiType::Void),
            platform_import(
                "GetStdHandle",
                "kernel32",
                &[AbiType::I32],
                AbiType::Pointer,
            ),
            platform_import(
                "VirtualAlloc",
                "kernel32",
                &[AbiType::Pointer, AbiType::USize, AbiType::U32, AbiType::U32],
                AbiType::Pointer,
            ),
            platform_import(
                "VirtualFree",
                "kernel32",
                &[AbiType::Pointer, AbiType::USize, AbiType::U32],
                AbiType::I32,
            ),
            platform_import(
                "WriteFile",
                "kernel32",
                &[
                    AbiType::Pointer,
                    AbiType::Pointer,
                    AbiType::U32,
                    AbiType::Pointer,
                    AbiType::Pointer,
                ],
                AbiType::I32,
            ),
        ],
        TargetTriple::Other(_) => Vec::new(),
    }
}

fn posix_platform_imports(library: &str) -> Vec<PlatformImport> {
    vec![
        platform_import("_exit", library, &[AbiType::I32], AbiType::Void),
        platform_import(
            "mmap",
            library,
            &[
                AbiType::Pointer,
                AbiType::USize,
                AbiType::I32,
                AbiType::I32,
                AbiType::I32,
                AbiType::I64,
            ],
            AbiType::Pointer,
        ),
        platform_import(
            "munmap",
            library,
            &[AbiType::Pointer, AbiType::USize],
            AbiType::I32,
        ),
        platform_import(
            "write",
            library,
            &[AbiType::I32, AbiType::Pointer, AbiType::USize],
            AbiType::ISize,
        ),
    ]
}

fn field(name: &str, offset: u64, ty: AbiType) -> AbiFieldLayout {
    AbiFieldLayout {
        name: name.into(),
        offset,
        ty,
    }
}

fn layout(name: &str, size: u64, alignment: u64, fields: Vec<AbiFieldLayout>) -> AbiLayout {
    AbiLayout {
        name: name.into(),
        size,
        alignment,
        fields,
    }
}

fn canonical_layouts(target: &TargetMetadata) -> Vec<AbiLayout> {
    let mut layouts = vec![
        layout(
            "BeskidAllocationRequest",
            24,
            8,
            vec![
                field("size", 0, AbiType::USize),
                field("alignment", 8, AbiType::USize),
                field("descriptor", 16, AbiType::Pointer),
            ],
        ),
        layout(
            "BeskidHandle",
            16,
            8,
            vec![
                field("slot", 0, AbiType::U32),
                field("generation", 4, AbiType::U32),
                field("owner_cookie", 8, AbiType::U64),
            ],
        ),
        layout(
            "BeskidObjectHeader",
            16,
            8,
            vec![
                field("descriptor", 0, AbiType::Pointer),
                field("gc_word", 8, AbiType::USize),
            ],
        ),
        layout(
            "BeskidRootFrame",
            24,
            8,
            vec![
                field("previous", 0, AbiType::Pointer),
                field("slots", 8, AbiType::Pointer),
                field("slot_count", 16, AbiType::USize),
            ],
        ),
        layout(
            "BeskidRootSlot",
            8,
            8,
            vec![field("value", 0, AbiType::Pointer)],
        ),
        layout(
            "BeskidRuntimeState",
            64,
            8,
            vec![
                field("abi_version", 0, AbiType::U32),
                field("flags", 4, AbiType::U32),
                field("current_thread", 8, AbiType::Pointer),
                field("heap", 16, AbiType::Pointer),
                field("handles", 24, AbiType::Pointer),
                field("scheduler", 32, AbiType::Pointer),
                field("root_frame", 40, AbiType::Pointer),
                field("tls_key", 48, AbiType::USize),
                field("corruption_cookie", 56, AbiType::U64),
            ],
        ),
        layout(
            "BeskidTlsState",
            32,
            8,
            vec![
                field("runtime", 0, AbiType::Pointer),
                field("root_frame", 8, AbiType::Pointer),
                field("current_fiber", 16, AbiType::Pointer),
                field("attach_depth", 24, AbiType::USize),
            ],
        ),
        layout(
            "BeskidTypeDescriptor",
            40,
            8,
            vec![
                field("size", 0, AbiType::USize),
                field("alignment", 8, AbiType::USize),
                field("pointer_map", 16, AbiType::Pointer),
                field("pointer_count", 24, AbiType::USize),
                field("flags", 32, AbiType::U32),
                field("reserved", 36, AbiType::U32),
            ],
        ),
    ];
    layouts.push(context_layout(target));
    layouts
}

fn context_layout(target: &TargetMetadata) -> AbiLayout {
    match target.triple {
        TargetTriple::X86_64UnknownLinuxGnu => layout(
            "BeskidArchContextX86_64SysV",
            64,
            16,
            ["rbx", "rbp", "r12", "r13", "r14", "r15", "rsp", "rip"]
                .into_iter()
                .enumerate()
                .map(|(index, name)| field(name, index as u64 * 8, AbiType::U64))
                .collect(),
        ),
        TargetTriple::Aarch64AppleDarwin => {
            let mut fields = (19..=30)
                .enumerate()
                .map(|(index, register)| {
                    field(&format!("x{register}"), index as u64 * 8, AbiType::U64)
                })
                .collect::<Vec<_>>();
            fields.push(field("sp", 96, AbiType::U64));
            fields.push(field("pc", 104, AbiType::U64));
            fields.extend((8..=15).enumerate().map(|(index, register)| {
                field(
                    &format!("d{register}"),
                    112 + index as u64 * 8,
                    AbiType::F64,
                )
            }));
            layout("BeskidArchContextAarch64Darwin", 176, 16, fields)
        }
        TargetTriple::X86_64PcWindowsMsvc => {
            let mut fields = ["rbx", "rbp", "rdi", "rsi", "r12", "r13", "r14", "r15"]
                .into_iter()
                .enumerate()
                .map(|(index, name)| field(name, index as u64 * 8, AbiType::U64))
                .collect::<Vec<_>>();
            fields.push(field("rsp", 64, AbiType::U64));
            fields.push(field("rip", 72, AbiType::U64));
            fields.extend((6..=15).enumerate().map(|(index, register)| {
                field(
                    &format!("xmm{register}"),
                    80 + index as u64 * 16,
                    AbiType::V128,
                )
            }));
            layout("BeskidArchContextX86_64Windows", 240, 16, fields)
        }
        TargetTriple::Other(_) => layout("UnsupportedContext", 1, 1, vec![]),
    }
}

fn macro_name(value: &str) -> String {
    value
        .chars()
        .enumerate()
        .flat_map(|(index, character)| {
            let separator = index > 0 && character.is_ascii_uppercase();
            separator
                .then_some('_')
                .into_iter()
                .chain(character.to_ascii_uppercase().to_string().chars())
                .collect::<Vec<_>>()
        })
        .collect()
}

pub fn render_runtime_c_header(
    manifest: &AbiManifestV5,
) -> Result<String, ManifestValidationError> {
    manifest.validate()?;
    let mut output = String::from(
        "/* @generated from the Beskid ABI-v5 manifest; do not edit. */\n#ifndef BESKID_RUNTIME_ABI_V5_H\n#define BESKID_RUNTIME_ABI_V5_H\n",
    );
    writeln!(output, "#define BESKID_RUNTIME_ABI_VERSION {ABI_V5}").unwrap();
    writeln!(output, "#define BESKID_TRAP_EXIT_STATUS {TRAP_EXIT_STATUS}").unwrap();
    writeln!(output, "#define BESKID_STACK_ALIGNMENT {STACK_ALIGNMENT}").unwrap();
    render_layout_defines(&mut output, manifest, "#define ", " ");
    for function in &manifest.exports {
        writeln!(
            output,
            "#define BESKID_SYMBOL_{} \"{}\"",
            macro_name(&function.symbol),
            function.symbol
        )
        .unwrap();
    }
    for export in &manifest.assembly_exports {
        writeln!(
            output,
            "{} {}({});",
            c_type(export.result),
            export.symbol.as_str(),
            export
                .params
                .iter()
                .map(|ty| c_type(*ty))
                .collect::<Vec<_>>()
                .join(", ")
        )
        .unwrap();
    }
    output.push_str("#endif\n");
    Ok(output)
}

pub fn render_runtime_asm_include(
    manifest: &AbiManifestV5,
) -> Result<String, ManifestValidationError> {
    manifest.validate()?;
    let mut output = String::from("# @generated from the Beskid ABI-v5 manifest; do not edit.\n");
    writeln!(output, "BESKID_RUNTIME_ABI_VERSION = {ABI_V5}").unwrap();
    writeln!(output, "BESKID_TRAP_EXIT_STATUS = {TRAP_EXIT_STATUS}").unwrap();
    writeln!(output, "BESKID_STACK_ALIGNMENT = {STACK_ALIGNMENT}").unwrap();
    writeln!(
        output,
        "BESKID_CALL_SHADOW_SPACE = {}",
        if matches!(target_triple(manifest), TargetTriple::X86_64PcWindowsMsvc) {
            32
        } else {
            0
        }
    )
    .unwrap();
    render_layout_defines(&mut output, manifest, "", " = ");
    let context = manifest
        .layouts
        .last()
        .expect("canonical layout has context");
    writeln!(output, "BESKID_ARCH_CONTEXT_SIZE = {}", context.size).unwrap();
    for export in &manifest.assembly_exports {
        let name = match export.symbol {
            super::AssemblySymbol::ContextInit => "CONTEXT_INIT",
            super::AssemblySymbol::ContextSwitch => "CONTEXT_SWITCH",
        };
        writeln!(
            output,
            "BESKID_{}_PARAM_COUNT = {}",
            name,
            export.params.len()
        )
        .unwrap();
        writeln!(
            output,
            "# signature ({}) -> {}",
            export
                .params
                .iter()
                .map(|ty| ty.canonical_name())
                .collect::<Vec<_>>()
                .join(", "),
            export.result.canonical_name()
        )
        .unwrap();
        writeln!(
            output,
            "BESKID_{name}_SYMBOL = {}{}",
            if matches!(target_triple(manifest), TargetTriple::Aarch64AppleDarwin) {
                "_"
            } else {
                ""
            },
            export.symbol.as_str()
        )
        .unwrap();
    }
    Ok(output)
}

fn c_type(ty: AbiType) -> &'static str {
    match ty {
        AbiType::Void => "void",
        AbiType::Pointer => "void *",
        AbiType::USize => "size_t",
        AbiType::ISize => "ptrdiff_t",
        AbiType::I8 => "int8_t",
        AbiType::U8 => "uint8_t",
        AbiType::I16 => "int16_t",
        AbiType::U16 => "uint16_t",
        AbiType::I32 => "int32_t",
        AbiType::U32 => "uint32_t",
        AbiType::I64 => "int64_t",
        AbiType::U64 => "uint64_t",
        AbiType::V128 => "beskid_v128_t",
        AbiType::F32 => "float",
        AbiType::F64 => "double",
    }
}

fn target_triple(manifest: &AbiManifestV5) -> &TargetTriple {
    &manifest.target.triple
}

fn render_layout_defines(
    output: &mut String,
    manifest: &AbiManifestV5,
    prefix: &str,
    separator: &str,
) {
    for layout in &manifest.layouts {
        let layout_name = macro_name(layout.name.strip_prefix("Beskid").unwrap_or(&layout.name));
        writeln!(
            output,
            "{prefix}BESKID_{layout_name}_SIZE{separator}{}",
            layout.size
        )
        .unwrap();
        writeln!(
            output,
            "{prefix}BESKID_{layout_name}_ALIGNMENT{separator}{}",
            layout.alignment
        )
        .unwrap();
        for field in &layout.fields {
            writeln!(
                output,
                "{prefix}BESKID_{layout_name}_{}_OFFSET{separator}{}",
                macro_name(&field.name),
                field.offset
            )
            .unwrap();
        }
    }
}
