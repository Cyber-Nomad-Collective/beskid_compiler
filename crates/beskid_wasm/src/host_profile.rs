//! Versioned descriptions of the final browser Wasm imports and exports.

use anyhow::{Context, Result, bail, ensure};
use beskid_queries::{SemanticTypeId, SourceItemSignature, SourceSignatureType};
use serde_json::{Value, json};
use wasmparser::{ExternalKind, FuncType, Parser, Payload, TypeRef, ValType, Validator, WasmFeatures};

pub(crate) const BYTE_HELPERS: [&str; 4] = ["array_i8_alloc", "array_i8_get", "array_i8_set", "array_i8_len"];
pub(crate) const WORD_HELPERS: [&str; 2] = ["array_i64_len", "array_i64_set"];

pub(crate) fn is_byte_buffer(ty: &SourceSignatureType) -> bool {
    matches!(ty, SourceSignatureType::Array(element) if **element == SourceSignatureType::Scalar(SemanticTypeId::U8))
}

pub(crate) fn is_managed(ty: &SourceSignatureType) -> bool {
    matches!(ty, SourceSignatureType::Nominal(_) | SourceSignatureType::Array(_) | SourceSignatureType::Function { .. })
        && !is_byte_buffer(ty)
}

pub(crate) fn append(
    bytes: &mut Vec<u8>,
    exports: &std::collections::BTreeMap<String, SourceItemSignature>,
    managed_scopes: bool,
    internal_exports: &[&str],
    entrypoint: &str,
) -> Result<()> {
    // Binaryen can remove imports and change the feature set of the final module.
    let features = WasmFeatures::WASM2 | WasmFeatures::GC | WasmFeatures::FUNCTION_REFERENCES;
    let types = Validator::new_with_features(features)
        .validate_all(bytes)
        .context("validating browser WebAssembly artifact")?;
    let types = types.as_ref();
    let mut services = Vec::new();
    let mut functions = std::collections::HashMap::new();
    let mut callees = Vec::new();
    let mut suspending = std::collections::HashSet::new();
    let mut spawn = None;
    let mut internal = std::collections::HashSet::new();
    let mut may_suspend = false;
    for payload in Parser::new(0).parse_all(bytes) {
        match payload? {
            Payload::ImportSection(imports) => {
                for import in imports.into_imports() {
                    let import = import?;
                    let TypeRef::Func(type_index) = import.ty else {
                        bail!("browser host import {}.{} must be a function", import.module, import.name);
                    };
                    let signature = types[types.core_type_at_in_module(type_index)].unwrap_func();
                    let (service, params, results, suspends) = host_service(import.name)?;
                    let namespace = if managed_scopes && suspends { crate::suspension::HOST_MODULE } else { "env" };
                    ensure!(
                        import.module == namespace,
                        "browser host import {}.{} has an unsupported namespace",
                        import.module,
                        import.name
                    );
                    check_signature(signature, params, results)
                        .with_context(|| format!("browser host import {} has an incompatible type", import.name))?;
                    if suspends { suspending.insert(callees.len() as u32); }
                    if import.name == crate::fibers::SPAWN { spawn = Some(callees.len() as u32); }
                    callees.push(Vec::new());
                    may_suspend |= suspends || import.name == crate::fibers::SPAWN;
                    services.push(json!({
                        "id": service,
                        "module": import.module,
                        "name": import.name,
                        "version": 1,
                        "parameters": params.iter().map(ValType::to_string).collect::<Vec<_>>(),
                        "results": results.iter().map(ValType::to_string).collect::<Vec<_>>(),
                        "required": true,
                        "maySuspend": suspends,
                    }));
                }
            }
            Payload::ExportSection(section) => {
                for export in section {
                    let export = export?;
                    if export.kind == ExternalKind::Func {
                        let signature = types[types.core_function_at(export.index)].unwrap_func();
                        if exports.contains_key(export.name) {
                            functions.insert(export.name, (signature, export.index));
                        } else {
                            ensure!(
                                internal_exports.contains(&export.name),
                                "unexpected browser function export {}",
                                export.name
                            );
                            let (params, results) = internal_signature(export.name)?;
                            check_signature(signature, params, results)
                                .with_context(|| format!("browser helper {}", export.name))?;
                            internal.insert(export.name);
                        }
                    }
                }
            }
            Payload::CodeSectionEntry(body) => {
                let mut calls = Vec::new();
                for op in body.get_operators_reader()? {
                    match op? {
                        wasmparser::Operator::Call { function_index } | wasmparser::Operator::ReturnCall { function_index } => calls.push(function_index),
                        wasmparser::Operator::CallIndirect { .. } | wasmparser::Operator::CallRef { .. } => {
                            if may_suspend { suspending.insert(callees.len() as u32); }
                        }
                        _ => {}
                    }
                }
                callees.push(calls);
            }
            Payload::StartSection { .. } => bail!("browser artifacts must not execute a Wasm start function"),
            Payload::CustomSection(section) if section.name() == "beskid.browser" => {
                bail!("duplicate browser artifact descriptor")
            }
            _ => {}
        }
    }

    ensure!(internal.len() == internal_exports.len(), "browser artifacts are missing required host helpers");
    let mut required_features = Vec::new();
    if Validator::new_with_features(WasmFeatures::WASM2).validate_all(bytes).is_err() {
        required_features.push("wasm-gc");
    }
    if may_suspend {
        required_features.push("jspi");
    }
    let mut identities = Vec::new();
    let mut values = |values: Vec<(&str, ValType)>, source_types: &[SourceSignatureType]| {
        values
            .into_iter()
            .zip(source_types)
            .map(|((kind, wasm_type), source_type)| {
                let mut value = json!({ "type": kind, "wasmType": wasm_type.to_string() });
                if kind == "managed" {
                    let id = identities.iter().position(|ty| ty == source_type).unwrap_or_else(|| {
                        identities.push(source_type.clone());
                        identities.len() - 1
                    });
                    value["typeId"] = json!(id);
                }
                value
            })
            .collect::<Vec<Value>>()
    };
    let mut export_descriptors = Vec::new();
    for (entry, source) in exports {
        let (function, index) = functions.get(entry.as_str()).with_context(|| format!("missing browser export {entry}"))?;
    let parameters = source.parameters.iter().map(browser_value).collect::<Result<Vec<_>>>()?;
    let results = if source.result == SourceSignatureType::Scalar(SemanticTypeId::UNIT) {
        Vec::new()
    } else {
        vec![browser_value(&source.result)?]
    };
    let wasm_types = |values: &[(&str, ValType)]| values.iter().map(|value| value.1).collect::<Vec<_>>();
    check_signature(function, &wasm_types(&parameters), &wasm_types(&results))
        .context("browser entrypoint has an incompatible Wasm type")?;

        let mut seen = std::collections::HashSet::new();
        let mut pending = vec![*index];
        let mut reachable_suspend = false;
        let mut reachable_spawn = false;
        while let Some(index) = pending.pop() {
            if !seen.insert(index) { continue; }
            reachable_suspend |= suspending.contains(&index);
            reachable_spawn |= spawn == Some(index);
            pending.extend_from_slice(&callees[index as usize]);
        }
        let await_children = entry == entrypoint && reachable_spawn;
        export_descriptors.push(json!({
            "name": entry,
            "parameters": values(parameters, &source.parameters),
            "results": values(results, std::slice::from_ref(&source.result)),
            "maySuspend": reachable_suspend || await_children,
            "awaitChildren": await_children,
        }));
    }
    let data = serde_json::to_vec(&json!({
        "formatVersion": 2,
        "hostInterfaceVersion": 2,
        "requiredFeatures": required_features,
        "internalExports": internal_exports,
        "managedScopes": managed_scopes,
        "exports": export_descriptors,
        "services": services,
    }))?;
    let profile = wasm_encoder::CustomSection { name: "beskid.browser".into(), data: data.into() };
    wasm_encoder::Section::append_to(&profile, bytes);
    Ok(())
}

fn browser_value(ty: &SourceSignatureType) -> Result<(&'static str, ValType)> {
    use SemanticTypeId as Ty;
    let SourceSignatureType::Scalar(ty) = ty else { return Ok((if is_byte_buffer(ty) { "bytes" } else { "managed" }, ValType::I64)) };
    let wasm_type = match *ty {
        Ty::BOOL | Ty::CHAR | Ty::I8 | Ty::I16 | Ty::U16 | Ty::I32 | Ty::U32 | Ty::U8 => ValType::I32,
        Ty::I64 | Ty::U64 | Ty::WORD | Ty::STRING => ValType::I64,
        Ty::F32 => ValType::F32,
        Ty::F64 => ValType::F64,
        _ => bail!("unsupported browser export value {ty}"),
    };
    Ok((ty.as_str(), wasm_type))
}

fn internal_signature(name: &str) -> Result<(&'static [ValType], &'static [ValType])> {
    use crate::arena::{SCOPE_GET, SCOPE_NEW, SCOPE_SET};
    use ValType::I32;
    const EQREF: ValType = ValType::Ref(wasmparser::RefType::EQREF);
    if name.strip_prefix("__beskid_fiber_entry_").is_some_and(|suffix| suffix.parse::<usize>().is_ok_and(|n| n > 0)) {
        return Ok((&[ValType::I64], &[ValType::I64]));
    }
    if let Some((entry, arity)) = name.strip_prefix("__beskid_composition_dispose_").and_then(|suffix| suffix.split_once('_')) {
        ensure!(entry.parse::<u32>().is_ok_and(|id| id > 0), "invalid browser composition hook entry");
        let arity = arity.parse::<usize>().context("invalid browser composition hook arity")?;
        const WORDS: [ValType; 2] = [ValType::I64; 2];
        return Ok((WORDS.get(..arity).context("invalid browser composition hook arity")?, &[]));
    }
    Ok(match name {
        crate::arena::REF_GET => (&[I32], &[EQREF]),
        crate::arena::REF_INTERN => (&[EQREF, I32], &[I32]),
        "array_i8_alloc" | "array_i8_len" => (&[I32], &[I32]),
        "array_i8_get" => (&[I32, I32], &[I32]),
        "array_i8_set" => (&[I32, I32, I32], &[]),
        "array_i64_len" => (&[I32], &[I32]),
        "array_i64_set" => (&[I32, I32, ValType::I64], &[]),
        crate::arena::FIBER_GET => (&[], &[ValType::I64]),
        crate::arena::FIBER_SET => (&[ValType::I64], &[]),
        SCOPE_GET | SCOPE_NEW => (&[], &[EQREF]),
        SCOPE_SET => (&[EQREF], &[]),
        "browser_string_new_len" | "browser_string_len" => (&[I32], &[I32]),
        "browser_string_assign" => (&[I32, I32], &[]),
        "browser_string_byte_set" => (&[I32, I32, I32], &[]),
        "browser_string_byte_get" => (&[I32, I32], &[I32]),
        _ => bail!("unknown internal browser helper {name}"),
    })
}

fn host_service(name: &str) -> Result<(&'static str, &'static [ValType], &'static [ValType], bool)> {
    use ValType::{F64, I64};
    if let Some(service) = crate::composition::service(name) {
        let (parameters, results) = service.profile();
        return Ok(("composition", parameters, results, false));
    }
    if let Some(service) = crate::network::service(name) {
        use ValType::I32;
        const STATUS: [ValType; 1] = [I32];
        const WORDS: [ValType; 6] = [I64; 6];
        const ADDRESS: [ValType; 3] = [I64, I32, I64];
        const DATAGRAM: [ValType; 5] = [I64, I64, I64, I32, I64];
        let (parameters, results) = service.wasm_shape(&STATUS, &WORDS, &ADDRESS, &DATAGRAM);
        return Ok(("network", parameters, results, service.suspends));
    }
    if let Some(service) = crate::scheduler::find(name) {
        let (parameters, results) = service.profile();
        return Ok(("scheduler", parameters, results, service.suspends));
    }
    if crate::stdio::handles(name) {
        return Ok(("streams", &[ValType::I32, I64, I64], &[I64], crate::stdio::READS.contains(&name)));
    }
    if let Some(operation) = beskid_abi::runtime_source::browser_protocol::operation(name) {
        const TYPES: [ValType; 8] = [I64; 8];
        let results = usize::from(operation.result != beskid_abi::runtime_source::browser_protocol::Value::Unit);
        return Ok((operation.capability, &TYPES[..operation.parameters.len()], &TYPES[..results], operation.suspends));
    }
    if let Some(service) = crate::foundation::service(name) {
        let params: &[ValType] = match service.args.len() { 0 => &[], 1 => &[I64], 2 => &[I64, I64], _ => bail!("invalid Foundation service arity") };
        let results: &[ValType] = if service.result == beskid_abi::runtime_source::CorelibServiceAbiType::I32 { &[ValType::I32] } else { &[I64] };
        return Ok((service.id, params, results, crate::foundation::FILES.contains(&name)));
    }
    Ok(match name {
        crate::managed_clif::dynamic::ISSUE_TOKEN | crate::managed_clif::dynamic::RESOLVE_TOKEN => ("dynamic", &[I64], &[I64], false),
        beskid_abi::SYM_CLOCK_MONOTONIC_NANOS
        | beskid_abi::SYM_CLOCK_REALTIME_NANOS
        | beskid_abi::SYM_FIBER_NOW_MILLIS => ("clock", &[], &[I64], false),
        crate::fibers::SPAWN => ("scheduler", &[I64, I64], &[I64], false),
        crate::fibers::JOIN_STATUS => ("scheduler", &[I64], &[ValType::I32], true),
        crate::fibers::DETACH | crate::fibers::STACK_ENTER | crate::fibers::STACK_LEAVE => ("scheduler", &[I64], &[], false),
        crate::fibers::CANCEL => ("scheduler", &[I64, I64], &[ValType::I32], false),
        crate::fibers::JOIN_VALUE | crate::fibers::JOIN_MESSAGE => ("scheduler", &[I64], &[I64], false),
        crate::fibers::JOIN_DETAIL => ("scheduler", &[I64, I64], &[I64], false),
        crate::fibers::JOIN_ERROR_FINISH => ("scheduler", &[I64], &[ValType::I32], false),
        crate::suspension::FIBER_YIELD => ("scheduler", &[], &[], true),
        crate::suspension::SLEEP_UNTIL => ("scheduler", &[I64], &[I64], true),
        beskid_abi::SYM_SYSCALL_WRITE => ("console", &[I64, I64], &[I64], false),
        crate::text_services::PANIC_MESSAGE | crate::text_services::PANIC_CODE => ("runtime", &[I64], &[], false),
        crate::text_services::FORMAT_F64 => ("runtime", &[I64], &[I64], false),
        "atan2" => ("math", &[F64, F64], &[F64], false),
        "sin" | "cos" | "tan" | "log" | "log2" | "log10" => ("math", &[F64], &[F64], false),
        _ => bail!("browser host service {name} is unavailable"),
    })
}

fn check_signature(signature: &FuncType, params: &[ValType], results: &[ValType]) -> Result<()> {
    ensure!(
        signature.params() == params && signature.results() == results,
        "expected {params:?} -> {results:?}, got {signature}"
    );
    Ok(())
}
