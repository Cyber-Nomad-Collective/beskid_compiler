use std::collections::BTreeMap;

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
    pub stack_alignment: u32,
    pub shadow_space: u32,
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
    pub symbol: String,
    pub capability: String,
    pub params: Vec<ParameterV5>,
    pub result: String,
    pub result_status: Option<String>,
    pub target_bindings: Vec<TargetAdapterBindingV5>,
}

/// Process-linked runtime operation outside the exact runtime-kit export surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SoftBuiltinV5 {
    pub name: String,
    pub symbol: String,
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
    #[serde(skip_serializing)]
    pub project_to_runtime: Option<String>,
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
#[serde(rename_all = "camelCase")]
pub struct CorelibServiceV5 {
    pub name: String,
    pub adapter: String,
    pub params: Vec<ParameterV5>,
    pub result: String,
    pub target_bindings: Vec<TargetAdapterBindingV5>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetAdapterBindingV5 {
    pub target: String,
    pub implementation: String,
    pub os_imports: Vec<String>,
}

/// Manifest-owned executable entry adapter for a Corelib service family.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryAdapterV5 {
    pub name: String,
    pub target: String,
    pub executable_entry: String,
    pub program_entry: String,
    pub capture: String,
    pub handoff: String,
    pub ownership: String,
    pub entry_source: String,
    pub os_imports: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AssemblyV5 {
    pub symbol: String,
    pub params: Vec<ParameterV5>,
    pub result: String,
    pub preserved: BTreeMap<String, Vec<String>>,
    pub locations: BTreeMap<String, Vec<ParameterLocationV5>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ParameterLocationV5 {
    Register { register: String },
    Stack { base: String, offset: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrapV5 {
    pub name: String,
    pub code: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusValueV5 {
    pub name: String,
    pub value: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusV5 {
    pub name: String,
    pub repr: String,
    pub values: Vec<StatusValueV5>,
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
    pub soft_builtins: Vec<SoftBuiltinV5>,
    pub layouts: Vec<LayoutV5>,
    pub platform_imports: Vec<PlatformImportV5>,
    pub corelib_services: Vec<CorelibServiceV5>,
    pub entry_adapters: Vec<EntryAdapterV5>,
    pub assembly: Vec<AssemblyV5>,
    pub traps: Vec<TrapV5>,
    pub statuses: Vec<StatusV5>,
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
