//! Optional `[Extern(..., Optional:true)]` contracts.
//!
//! A host binds an optional symbol that did not resolve to a null address: the JIT binds the
//! name to null, and AOT declares the symbol as a weak undefined reference. Generated code must
//! never call such an address, so this pass
//!
//! - routes every reference to an optional symbol through a generated thunk that loads the
//!   symbol's address, calls it when it is not null, and otherwise raises the ABI-v5
//!   `extern_unavailable` trap through `beskid_rt_v5_trap`; and
//! - defines each called `bool Available()` query as the conjunction of "address is not null"
//!   over every C symbol of its contract.

use std::collections::{BTreeMap, HashMap};

use cranelift_codegen::ir::{
    AbiParam, ExtFuncData, ExternalName, Function, GlobalValueData, InstBuilder, Signature, TrapCode, UserFuncName,
    condcodes::IntCC, types,
};
use cranelift_codegen::isa::TargetIsa;
use cranelift_codegen::verify_function;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};

use super::contracts::{SyntaxModuleEmissionError, emission_verification};
use super::imports::AvailabilityQuery;
use crate::{CodegenContext, CodegenInput, ExternImport, LoweredFunction};

/// Runtime-kit export that terminates the process with a typed ABI-v5 trap.
const RUNTIME_TRAP_SYMBOL: &str = "beskid_rt_v5_trap";

/// Address a host may bind to an optional symbol that did not resolve, instead of null. Some
/// relocation forms cannot encode a null target (cranelift-jit's x86-64 GOT relocation), so the
/// JIT binds absent symbols to this sentinel's address. AOT leaves it a weak undefined reference,
/// which resolves to null. Generated code treats both null and this address as "absent".
pub const OPTIONAL_EXTERN_ABSENT_SYMBOL: &str = "beskid_optional_extern_absent";

/// `address != 0 && address != &OPTIONAL_EXTERN_ABSENT_SYMBOL`, as an `i8`.
fn resolved(
    builder: &mut FunctionBuilder<'_>,
    isa: &dyn TargetIsa,
    absent: cranelift_codegen::ir::FuncRef,
    address: cranelift_codegen::ir::Value,
) -> cranelift_codegen::ir::Value {
    let pointer = isa.pointer_type();
    let absent = builder.ins().func_addr(pointer, absent);
    let non_null = builder.ins().icmp_imm_u(IntCC::NotEqual, address, 0);
    let not_absent = builder.ins().icmp(IntCC::NotEqual, address, absent);
    builder.ins().band(non_null, not_absent)
}

fn import_absent(builder: &mut FunctionBuilder<'_>, isa: &dyn TargetIsa) -> cranelift_codegen::ir::FuncRef {
    import(builder, OPTIONAL_EXTERN_ABSENT_SYMBOL, Signature::new(isa.default_call_conv()))
}

fn thunk_symbol(symbol: &str) -> String {
    format!("__beskid_optional_extern_{symbol}")
}

/// Add the thunks and availability queries an artifact with optional externs needs. `functions`
/// holds every lowered function; `extern_imports` gains each availability member and the
/// runtime trap import.
pub(super) fn emit_optional_extern_support(
    input: &CodegenInput<'_>,
    isa: &dyn TargetIsa,
    functions: &mut Vec<LoweredFunction>,
    extern_imports: &mut Vec<ExternImport>,
    availability: &[AvailabilityQuery],
    context: &mut CodegenContext,
) -> Result<(), SyntaxModuleEmissionError> {
    for query in availability {
        for (_, member) in &query.members {
            if !extern_imports.iter().any(|existing| existing.symbol == member.symbol) {
                extern_imports.push(member.clone());
            }
        }
    }
    let optional = extern_imports
        .iter()
        .filter(|import| import.optional)
        .map(|import| (import.symbol.clone(), import.clone()))
        .collect::<BTreeMap<_, _>>();
    if optional.is_empty() {
        return Ok(());
    }
    if !extern_imports.iter().any(|existing| existing.symbol == OPTIONAL_EXTERN_ABSENT_SYMBOL) {
        extern_imports.push(ExternImport {
            symbol: OPTIONAL_EXTERN_ABSENT_SYMBOL.to_owned(),
            abi: Some("C".into()),
            library: None,
            optional: true,
        });
    }

    // One call signature per optional symbol, taken from its references; then send every
    // reference through the symbol's thunk.
    let mut call_signatures: BTreeMap<String, Signature> = BTreeMap::new();
    for function in functions.iter_mut() {
        let renames = function
            .function
            .dfg
            .ext_funcs
            .iter()
            .filter_map(|(func_ref, external)| {
                let ExternalName::TestCase(name) = &external.name else {
                    return None;
                };
                let symbol = String::from_utf8_lossy(name.raw()).into_owned();
                optional.contains_key(&symbol).then_some((func_ref, symbol, external.signature))
            })
            .collect::<Vec<_>>();
        for (func_ref, symbol, signature) in renames {
            let signature = function.function.dfg.signatures[signature].clone();
            match call_signatures.get(&symbol) {
                Some(existing) if existing != &signature => {
                    return Err(emission_verification(format!(
                        "optional extern `{symbol}` is referenced with two different signatures"
                    )));
                }
                Some(_) => {}
                None => {
                    call_signatures.insert(symbol.clone(), signature);
                }
            }
            function.function.dfg.ext_funcs[func_ref].name = ExternalName::testcase(thunk_symbol(&symbol).as_bytes());
        }
    }

    let trap_code = beskid_abi::abi_v5::TrapCode::all()
        .into_iter()
        .find(|trap| trap.name == beskid_abi::interop::c_profile::OPTIONAL_EXTERN_UNAVAILABLE_TRAP)
        .map(|trap| trap.code)
        .ok_or_else(|| emission_verification("ABI-v5 manifest has no `extern_unavailable` trap"))?;
    for (symbol, signature) in &call_signatures {
        let import = &optional[symbol];
        let message = format!(
            "optional extern `{symbol}` from `{}` is unavailable; check `Available()` before calling it",
            import.library.as_deref().unwrap_or("<no library>")
        );
        let message_symbol = context.intern_string_literal(message.as_bytes());
        let function = emit_thunk(isa, symbol, signature, trap_code, &message_symbol, message.len())?;
        functions.push(LoweredFunction { name: thunk_symbol(symbol), function });
    }
    if !call_signatures.is_empty() && !extern_imports.iter().any(|existing| existing.symbol == RUNTIME_TRAP_SYMBOL) {
        extern_imports.push(ExternImport {
            symbol: RUNTIME_TRAP_SYMBOL.to_owned(),
            abi: Some("C".into()),
            library: None,
            optional: false,
        });
    }

    for query in availability {
        let Some(query_signature) = functions.iter().find_map(|function| {
            function.function.dfg.ext_funcs.values().find_map(|external| match &external.name {
                ExternalName::TestCase(name) if name.raw() == query.symbol.as_bytes() => {
                    Some(function.function.dfg.signatures[external.signature].clone())
                }
                _ => None,
            })
        }) else {
            // No lowered function calls this query.
            continue;
        };
        let mut members = Vec::with_capacity(query.members.len());
        for (key, member) in &query.members {
            let signature = match call_signatures.get(&member.symbol) {
                Some(signature) => signature.clone(),
                None => member_signature(input, isa, *key, &member.symbol)?,
            };
            members.push((member.symbol.as_str(), signature));
        }
        let function = emit_availability_query(isa, &query.symbol, &query_signature, &members)?;
        functions.push(LoweredFunction { name: query.symbol.clone(), function });
    }
    Ok(())
}

fn member_signature(
    input: &CodegenInput<'_>,
    isa: &dyn TargetIsa,
    key: beskid_isle::AstNodeKey,
    symbol: &str,
) -> Result<Signature, SyntaxModuleEmissionError> {
    beskid_queries::item_abi_signature(input.database(), key)
        .ok()
        .flatten()
        .and_then(|signature| crate::isle_adapter::mappings::signature_for_item(isa, signature))
        .ok_or_else(|| emission_verification(format!("optional extern `{symbol}` has no C ABI signature")))
}

fn import(builder: &mut FunctionBuilder<'_>, symbol: &str, signature: Signature) -> cranelift_codegen::ir::FuncRef {
    let signature = builder.import_signature(signature);
    builder.func.import_function(ExtFuncData {
        name: ExternalName::testcase(symbol.as_bytes()),
        signature,
        colocated: false,
        patchable: false,
    })
}

/// `symbol`'s thunk: call through the resolved address, or trap when it is null.
fn emit_thunk(
    isa: &dyn TargetIsa,
    symbol: &str,
    signature: &Signature,
    trap_code: u8,
    message_symbol: &str,
    message_len: usize,
) -> Result<Function, SyntaxModuleEmissionError> {
    let pointer = isa.pointer_type();
    let mut function = Function::with_name_signature(UserFuncName::user(0, 0), signature.clone());
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
        let entry = builder.create_block();
        let resolved_block = builder.create_block();
        let unavailable = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        let arguments = builder.block_params(entry).to_vec();
        let target = import(&mut builder, symbol, signature.clone());
        let address = builder.ins().func_addr(pointer, target);
        let absent = import_absent(&mut builder, isa);
        let available = resolved(&mut builder, isa, absent, address);
        builder.ins().brif(available, resolved_block, &[], unavailable, &[]);

        builder.switch_to_block(resolved_block);
        let call_signature = builder.import_signature(signature.clone());
        let call = builder.ins().call_indirect(call_signature, address, &arguments);
        let results = builder.inst_results(call).to_vec();
        builder.ins().return_(&results);

        builder.switch_to_block(unavailable);
        let mut trap_signature = Signature::new(isa.default_call_conv());
        trap_signature.params.extend([AbiParam::new(types::I8), AbiParam::new(pointer), AbiParam::new(pointer)]);
        let trap = import(&mut builder, RUNTIME_TRAP_SYMBOL, trap_signature);
        let code = builder.ins().iconst(types::I8, i64::from(trap_code));
        let message = builder.func.create_global_value(GlobalValueData::Symbol {
            name: ExternalName::testcase(message_symbol.as_bytes()),
            offset: 0.into(),
            colocated: false,
            tls: false,
        });
        let message = builder.ins().symbol_value(pointer, message);
        let length = builder.ins().iconst(pointer, message_len as i64);
        builder.ins().call(trap, &[code, message, length]);
        // `beskid_rt_v5_trap` does not return.
        builder.ins().trap(TrapCode::unwrap_user(9));
        builder.seal_all_blocks();
        builder.finalize(isa.frontend_config());
    }
    verify_function(&function, isa.flags()).map_err(|error| {
        emission_verification(format!("optional extern thunk for `{symbol}` failed verification: {error}"))
    })?;
    Ok(function)
}

/// `bool Available()`: true only when every member symbol resolved to a non-null address.
fn emit_availability_query(
    isa: &dyn TargetIsa,
    query_symbol: &str,
    signature: &Signature,
    members: &[(&str, Signature)],
) -> Result<Function, SyntaxModuleEmissionError> {
    let [result] = signature.returns.as_slice() else {
        return Err(emission_verification(format!("availability query `{query_symbol}` must return one bool")));
    };
    if !signature.params.is_empty() || !result.value_type.is_int() {
        return Err(emission_verification(format!("availability query `{query_symbol}` must be `bool Available()`")));
    }
    let pointer = isa.pointer_type();
    let mut function = Function::with_name_signature(UserFuncName::user(0, 0), signature.clone());
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
        let entry = builder.create_block();
        builder.switch_to_block(entry);
        builder.seal_block(entry);
        let absent = import_absent(&mut builder, isa);
        let mut available = builder.ins().iconst(types::I8, 1);
        let mut imported = HashMap::new();
        for (symbol, member_signature) in members {
            let target = *imported
                .entry(*symbol)
                .or_insert_with(|| import(&mut builder, symbol, member_signature.clone()));
            let address = builder.ins().func_addr(pointer, target);
            let member_resolved = resolved(&mut builder, isa, absent, address);
            available = builder.ins().band(available, member_resolved);
        }
        let value = match result.value_type {
            types::I8 => available,
            wider => builder.ins().uextend(wider, available),
        };
        builder.ins().return_(&[value]);
        builder.finalize(isa.frontend_config());
    }
    verify_function(&function, isa.flags()).map_err(|error| {
        emission_verification(format!("availability query `{query_symbol}` failed verification: {error}"))
    })?;
    Ok(function)
}
