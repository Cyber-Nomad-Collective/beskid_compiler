use std::collections::BTreeMap;

use serde::Deserialize;

use super::super::{
    AbiFieldLayout, AbiFunction, AbiLayout, AbiType, AssemblyExport, AssemblyParameterLocation, AssemblyRegister,
    AssemblySymbol, PlatformImport, RuntimeIntrinsic, RuntimeTargetBinding,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SourceContract {
    pub(super) targets: Vec<SourceTarget>,
    pub(super) exports: Vec<SourceFunction>,
    pub(super) intrinsics: Vec<SourceIntrinsic>,
    #[serde(rename = "softBuiltins")]
    _soft_builtins: Vec<SourceSoftBuiltin>,
    pub(super) layouts: Vec<SourceLayout>,
    pub(super) platform_imports: Vec<SourcePlatformImport>,
    pub(super) assembly: Vec<SourceAssembly>,
    #[serde(rename = "corelibServices")]
    _corelib_services: serde_json::Value,
    #[serde(rename = "entryAdapters")]
    _entry_adapters: serde_json::Value,
    #[serde(rename = "traps")]
    _traps: serde_json::Value,
    #[serde(rename = "meta")]
    _meta: serde_json::Value,
    #[serde(rename = "audit")]
    _audit: serde_json::Value,
    #[serde(default)]
    _statuses: Vec<SourceStatus>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SourceTarget {
    pub(super) triple: String,
    #[serde(rename = "endianness")]
    _endianness: String,
    #[serde(rename = "pointerWidth")]
    _pointer_width: u8,
    #[serde(rename = "callingConvention")]
    _calling_convention: String,
    #[serde(rename = "objectFormat")]
    _object_format: String,
    #[serde(rename = "symbolPrefix")]
    _symbol_prefix: String,
    #[serde(rename = "stackAlignment")]
    _stack_alignment: u32,
    #[serde(rename = "shadowSpace")]
    _shadow_space: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceParameter {
    name: String,
    #[serde(rename = "type")]
    ty: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceFunction {
    symbol: String,
    params: Vec<SourceParameter>,
    result: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceIntrinsic {
    name: String,
    symbol: String,
    capability: String,
    params: Vec<SourceParameter>,
    result: String,
    #[serde(default, rename = "resultStatus")]
    _result_status: Option<String>,
    #[serde(default, rename = "targetBindings")]
    target_bindings: Vec<SourceTargetBinding>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceTargetBinding {
    #[serde(rename = "target")]
    target: String,
    #[serde(rename = "implementation")]
    implementation: String,
    #[serde(rename = "osImports")]
    os_imports: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceSoftBuiltin {
    #[serde(rename = "name")]
    _name: String,
    #[serde(rename = "symbol")]
    _symbol: String,
    #[serde(rename = "params")]
    _params: Vec<SourceParameter>,
    #[serde(rename = "result")]
    _result: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceStatus {
    #[serde(rename = "name")]
    _name: String,
    #[serde(rename = "repr")]
    _repr: String,
    #[serde(rename = "values")]
    _values: Vec<SourceStatusValue>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceStatusValue {
    #[serde(rename = "name")]
    _name: String,
    #[serde(rename = "value")]
    _value: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceField {
    name: String,
    offset: u64,
    #[serde(rename = "type")]
    ty: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceLayout {
    name: String,
    pub(super) target: Option<String>,
    size: u64,
    alignment: u64,
    fields: Vec<SourceField>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SourcePlatformImport {
    symbol: String,
    pub(super) target: String,
    library: String,
    params: Vec<SourceParameter>,
    result: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceAssembly {
    symbol: String,
    params: Vec<SourceParameter>,
    result: String,
    preserved: BTreeMap<String, Vec<String>>,
    locations: BTreeMap<String, Vec<SourceParameterLocation>>,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum SourceParameterLocation {
    Register { register: String },
    Stack { base: String, offset: u64 },
}

fn source_type(value: &str) -> AbiType {
    crate::generated::abi_v5_contract::ABI_V5_TYPES
        .iter()
        .find_map(|(name, ty)| (*name == value).then_some(*ty))
        .expect("build validates ABI types")
}
fn source_params(params: &[SourceParameter]) -> (Vec<String>, Vec<AbiType>) {
    (
        params.iter().map(|entry| entry.name.clone()).collect(),
        params.iter().map(|entry| source_type(&entry.ty)).collect(),
    )
}
pub(super) fn source_function(entry: &SourceFunction) -> AbiFunction {
    let (param_names, params) = source_params(&entry.params);
    AbiFunction {
        symbol: entry.symbol.clone(),
        param_names,
        params,
        result: source_type(&entry.result),
        noreturn: entry.result == "never",
    }
}
pub(super) fn source_intrinsic(entry: &SourceIntrinsic) -> RuntimeIntrinsic {
    let (param_names, params) = source_params(&entry.params);
    RuntimeIntrinsic {
        name: entry.name.clone(),
        symbol: entry.symbol.clone(),
        capability: entry.capability.clone(),
        param_names,
        params,
        result: source_type(&entry.result),
        noreturn: entry.result == "never",
        target_bindings: entry
            .target_bindings
            .iter()
            .map(|binding| RuntimeTargetBinding {
                target: binding.target.clone(),
                implementation: binding.implementation.clone(),
                os_imports: binding.os_imports.clone(),
            })
            .collect(),
    }
}
pub(super) fn source_platform_import(entry: &SourcePlatformImport) -> PlatformImport {
    let (param_names, params) = source_params(&entry.params);
    PlatformImport {
        symbol: entry.symbol.clone(),
        library: entry.library.clone(),
        param_names,
        params,
        result: source_type(&entry.result),
        noreturn: entry.result == "never",
    }
}
pub(super) fn source_layout(entry: &SourceLayout) -> AbiLayout {
    AbiLayout {
        name: entry.name.clone(),
        size: entry.size,
        alignment: entry.alignment,
        fields: entry
            .fields
            .iter()
            .map(|field| AbiFieldLayout { name: field.name.clone(), offset: field.offset, ty: field.ty.clone() })
            .collect(),
    }
}
fn source_register(value: &str) -> AssemblyRegister {
    AssemblyRegister::new(value)
}
pub(super) fn source_assembly(entry: &SourceAssembly, target: &str) -> AssemblyExport {
    let (param_names, params) = source_params(&entry.params);
    AssemblyExport {
        symbol: AssemblySymbol::new(&entry.symbol),
        param_names,
        params,
        parameter_locations: entry.locations[target]
            .iter()
            .map(|location| match location {
                SourceParameterLocation::Register { register } => {
                    AssemblyParameterLocation::Register { register: AssemblyRegister::new(register) }
                }
                SourceParameterLocation::Stack { base, offset } => {
                    AssemblyParameterLocation::Stack { base: AssemblyRegister::new(base), offset: *offset }
                }
            })
            .collect(),
        result: source_type(&entry.result),
        preserved_registers: entry.preserved[target].iter().map(|value| source_register(value)).collect(),
    }
}
