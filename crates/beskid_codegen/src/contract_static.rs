//! Static data of contract values (runtime contract dispatch, v0.6.7).
//!
//! A contract box is a 32-byte managed record: `{ header, receiver word, witness table address }`.
//! Two descriptors serve every box of a module: the reference box traces its receiver word, the
//! scalar box (a boxed primitive) traces nothing. A witness table is a module-local array of
//! function addresses, one per contract method in slot order; a scalar receiver's entries are
//! thunks that narrow the receiver word to the primitive and call the implementation.

use beskid_queries::{ItemSignature, SemanticTypeId};
use cranelift_codegen::ir::{ExtFuncData, ExternalName, Function, InstBuilder, MemFlagsData, Signature, Type, types};
use cranelift_codegen::isa::TargetIsa;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{DataDescription, FuncId, Linkage, Module, ModuleError, ModuleResult};

use crate::isle_adapter::mappings::{map_scalar_type, signature_for_item};

pub const CONTRACT_BOX_REFERENCE_POINTER_MAP_SYMBOL: &str = "__beskid_contract_box_reference_pointer_map_v5";
pub const CONTRACT_BOX_REFERENCE_DESCRIPTOR_SYMBOL: &str = "__beskid_contract_box_reference_descriptor_v5";
pub const CONTRACT_BOX_REFERENCE_REQUEST_SYMBOL: &str = "__beskid_contract_box_reference_request_v5";
pub const CONTRACT_BOX_SCALAR_POINTER_MAP_SYMBOL: &str = "__beskid_contract_box_scalar_pointer_map_v5";
pub const CONTRACT_BOX_SCALAR_DESCRIPTOR_SYMBOL: &str = "__beskid_contract_box_scalar_descriptor_v5";
pub const CONTRACT_BOX_SCALAR_REQUEST_SYMBOL: &str = "__beskid_contract_box_scalar_request_v5";
pub const CONTRACT_BOX_SIZE: u64 = beskid_queries::CONTRACT_BOX_SIZE;
pub const CONTRACT_BOX_RECEIVER_OFFSET: u64 = beskid_queries::CONTRACT_BOX_RECEIVER_OFFSET as u64;

/// One witness table: the module symbols of its entries in slot order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WitnessTablePlan {
    pub symbol: String,
    pub entries: Vec<String>,
}

/// An unboxing thunk: `(word receiver, params...) -> result` narrows the receiver word to the
/// primitive `receiver` and calls `target_symbol`, a method of `impl <primitive> : C`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WitnessThunkPlan {
    pub symbol: String,
    pub target_symbol: String,
    pub receiver: SemanticTypeId,
    pub target_signature: ItemSignature,
}

/// Define the two box descriptors and their allocation requests.
pub fn emit_contract_box_static_data<M: Module>(module: &mut M) -> ModuleResult<()> {
    emit_box_descriptor(
        module,
        CONTRACT_BOX_REFERENCE_POINTER_MAP_SYMBOL,
        CONTRACT_BOX_REFERENCE_DESCRIPTOR_SYMBOL,
        CONTRACT_BOX_REFERENCE_REQUEST_SYMBOL,
        Some(CONTRACT_BOX_RECEIVER_OFFSET),
    )?;
    emit_box_descriptor(
        module,
        CONTRACT_BOX_SCALAR_POINTER_MAP_SYMBOL,
        CONTRACT_BOX_SCALAR_DESCRIPTOR_SYMBOL,
        CONTRACT_BOX_SCALAR_REQUEST_SYMBOL,
        None,
    )
}

fn emit_box_descriptor<M: Module>(
    module: &mut M,
    pointer_map_symbol: &str,
    descriptor_symbol: &str,
    request_symbol: &str,
    traced_offset: Option<u64>,
) -> ModuleResult<()> {
    let pointer_map = module.declare_data(pointer_map_symbol, Linkage::Local, false, false)?;
    let descriptor = module.declare_data(descriptor_symbol, Linkage::Local, false, false)?;
    let request = module.declare_data(request_symbol, Linkage::Local, false, false)?;

    let mut pointer_map_data = DataDescription::new();
    pointer_map_data.define(traced_offset.unwrap_or(0).to_le_bytes().to_vec().into_boxed_slice());
    module.define_data(pointer_map, &pointer_map_data)?;

    let mut descriptor_bytes = vec![0u8; 40];
    write_word(&mut descriptor_bytes, 0, CONTRACT_BOX_SIZE)?;
    write_word(&mut descriptor_bytes, 8, 8)?;
    write_word(&mut descriptor_bytes, 24, u64::from(traced_offset.is_some()))?;
    let mut descriptor_data = DataDescription::new();
    descriptor_data.define(descriptor_bytes.into_boxed_slice());
    if traced_offset.is_some() {
        let pointer_map_address = module.declare_data_in_data(pointer_map, &mut descriptor_data);
        descriptor_data.write_data_addr(16, pointer_map_address, 0);
    }
    module.define_data(descriptor, &descriptor_data)?;

    let mut request_bytes = vec![0u8; 24];
    write_word(&mut request_bytes, 0, CONTRACT_BOX_SIZE)?;
    write_word(&mut request_bytes, 8, 8)?;
    let mut request_data = DataDescription::new();
    request_data.define(request_bytes.into_boxed_slice());
    let descriptor_address = module.declare_data_in_data(descriptor, &mut request_data);
    request_data.write_data_addr(16, descriptor_address, 0);
    module.define_data(request, &request_data)
}

/// Define every witness table of `plans`, after the functions they name are declared.
/// `function_id` maps an entry's module symbol to its declared function.
pub fn emit_witness_tables<M: Module>(
    module: &mut M,
    plans: &[WitnessTablePlan],
    function_id: &dyn Fn(&str) -> Option<FuncId>,
) -> ModuleResult<()> {
    for plan in plans {
        let table = module.declare_data(&plan.symbol, Linkage::Local, false, false)?;
        let mut data = DataDescription::new();
        data.define(vec![0u8; plan.entries.len().max(1) * 8].into_boxed_slice());
        for (slot, entry) in plan.entries.iter().enumerate() {
            let id = function_id(entry).ok_or_else(|| {
                ModuleError::Backend(anyhow::anyhow!("witness table `{}` names an undeclared function `{entry}`", plan.symbol))
            })?;
            let reference = module.declare_func_in_data(id, &mut data);
            let offset = u32::try_from(slot * 8)
                .map_err(|_| ModuleError::Backend(anyhow::anyhow!("witness table `{}` exceeds the ABI word", plan.symbol)))?;
            data.write_function_addr(offset, reference);
        }
        module.define_data(table, &data)?;
    }
    Ok(())
}

/// The Cranelift signature of a witness table entry: the receiver word first.
pub fn witness_entry_signature(isa: &dyn TargetIsa, target: &ItemSignature) -> Option<Signature> {
    let pointer = isa.pointer_type();
    let mut signature = signature_for_item(isa, target.clone())?;
    let first = signature.params.first_mut()?;
    first.value_type = pointer;
    Some(signature)
}

/// Build the body of one unboxing thunk.
pub fn emit_witness_thunk(plan: &WitnessThunkPlan, isa: &dyn TargetIsa) -> Option<Function> {
    let pointer = isa.pointer_type();
    let signature = witness_entry_signature(isa, &plan.target_signature)?;
    let target_signature = signature_for_item(isa, plan.target_signature.clone())?;
    let receiver_type = target_signature.params.first()?.value_type;
    let mut function = Function::with_name_signature(cranelift_codegen::ir::UserFuncName::user(0, 0), signature);
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);
        let params = builder.block_params(entry).to_vec();
        let (word, rest) = params.split_first()?;
        let receiver = narrow_receiver(&mut builder, *word, receiver_type, pointer)?;
        let imported = builder.import_signature(target_signature);
        let target = builder.func.import_function(ExtFuncData {
            name: ExternalName::testcase(plan.target_symbol.as_bytes()),
            signature: imported,
            colocated: false,
            patchable: false,
        });
        let mut arguments = Vec::with_capacity(rest.len() + 1);
        arguments.push(receiver);
        arguments.extend(rest.iter().copied());
        let call = builder.ins().call(target, &arguments);
        let results = builder.inst_results(call).to_vec();
        builder.ins().return_(&results);
        builder.finalize(isa.frontend_config());
    }
    Some(function)
}

/// Narrow the receiver word back to the primitive the implementation takes by value.
fn narrow_receiver(builder: &mut FunctionBuilder<'_>, word: cranelift_codegen::ir::Value, receiver: Type, pointer: Type) -> Option<cranelift_codegen::ir::Value> {
    if receiver == pointer {
        return Some(word);
    }
    if receiver.is_float() {
        let int = Type::int(u16::try_from(receiver.bits()).ok()?)?;
        let bits = if int == pointer { word } else { builder.ins().ireduce(int, word) };
        return Some(builder.ins().bitcast(receiver, MemFlagsData::new(), bits));
    }
    if receiver.is_int() && receiver.bits() < pointer.bits() {
        return Some(builder.ins().ireduce(receiver, word));
    }
    (receiver == pointer || receiver == types::I64).then_some(word)
}

/// The Cranelift type of a scalar receiver's primitive, for the box's receiver fact.
pub fn scalar_receiver_type(receiver: SemanticTypeId) -> Option<Type> {
    map_scalar_type(receiver)
}

fn write_word(bytes: &mut [u8], offset: usize, value: u64) -> Result<(), ModuleError> {
    let destination = bytes
        .get_mut(offset..offset + 8)
        .ok_or_else(|| ModuleError::Backend(anyhow::anyhow!("contract box static-data layout overflow")))?;
    destination.copy_from_slice(&value.to_le_bytes());
    Ok(())
}
