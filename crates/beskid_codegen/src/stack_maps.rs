//! Emit precise Cranelift stack maps and publish their locations during calls.

#![allow(non_snake_case)]

use std::collections::HashMap;

use cranelift_codegen::Context;
use cranelift_codegen::cursor::{Cursor, FuncCursor};
use cranelift_codegen::ir::{AbiParam, Endianness, InstBuilder, StackSlotData, StackSlotKind, types};
use cranelift_module::{
    DataDescription, FuncId, FuncOrDataId, Linkage, Module, ModuleError, ModuleRelocTarget, ModuleResult,
};

pub(crate) const STACK_MAP_PUSH: &str = "beskid_rt_v5_stack_map_push";
pub(crate) const STACK_MAP_POP: &str = "beskid_rt_v5_stack_map_pop";

const TABLE_MAGIC: &[u8; 8] = b"BSKGC\0\x01\0";
const TABLE_LENGTH_OFFSET: usize = 8;
const FUNCTION_ADDRESS_OFFSET: u32 = 16;
pub(crate) const CHECKPOINT_BYTES: u32 = 32;
pub(crate) const CHECKPOINT_ALIGNMENT: u32 = 8;
pub(crate) const CHECKPOINT_VALUES_PER_CALL: u32 = 3;

/// Logical names of checkpoint helpers lowered from proven canonical runtime source.
#[derive(Debug, Clone, Default)]
pub struct RuntimeCheckpointDefinitions {
    pub(crate) push: Option<String>,
    pub(crate) pop: Option<String>,
}

/// Function ids for the canonical helpers when defined in the current module.
#[derive(Clone, Copy, Default)]
pub struct RuntimeCheckpointFunctions {
    push: Option<FuncId>,
    pop: Option<FuncId>,
}

impl RuntimeCheckpointDefinitions {
    pub fn Resolve(&self, functionIds: &HashMap<String, FuncId>) -> RuntimeCheckpointFunctions {
        RuntimeCheckpointFunctions {
            push: self.push.as_ref().and_then(|name| functionIds.get(name)).copied(),
            pop: self.pop.as_ref().and_then(|name| functionIds.get(name)).copied(),
        }
    }
}

/// Define a function with a checkpoint around each call that has live stack-map roots.
///
/// The finalized CLIF already stores the roots before each mapped call. Giving
/// the preceding push call the same map lets the runtime identify those stores
/// from its own return address. The runtime retains that map and the caller's
/// stack pointer until the target call returns, including across fiber switches.
/// The helpers come from proven definitions in the current module or the exact ABI-v5 runtime kit.
pub fn DefineFunctionWithStackMaps<M: Module>(
    module: &mut M,
    functionId: FuncId,
    context: &mut Context,
    section: Option<&str>,
    checkpoints: RuntimeCheckpointFunctions,
) -> ModuleResult<()> {
    let mappedCalls = context
        .func
        .layout
        .blocks()
        .flat_map(|block| context.func.layout.block_insts(block))
        .filter_map(|inst| {
            let roots = context.func.dfg.user_stack_map_entries(inst)?;
            (!roots.is_empty()).then(|| (inst, roots.to_vec()))
        })
        .collect::<Vec<_>>();
    if mappedCalls.is_empty() {
        return module.define_function(functionId, context);
    }

    let pointer = module.isa().pointer_type();
    if pointer != types::I64 {
        return Err(ModuleError::Backend(anyhow::anyhow!("Beskid GC stack maps require a 64-bit target")));
    }
    let pointerSize = pointer.bytes();
    for (call, roots) in &mappedCalls {
        let opcode = context.func.dfg.insts[*call].opcode();
        if opcode.is_terminator() {
            return Err(ModuleError::Backend(anyhow::anyhow!("cannot publish a GC stack map for {opcode}")));
        }
        if roots.iter().any(|root| root.ty != pointer) {
            return Err(ModuleError::Backend(anyhow::anyhow!("Beskid GC stack map contains a non-pointer slot")));
        }
    }

    let tableId = module.declare_anonymous_data(false, false)?;
    let table = module.declare_data_in_func(tableId, &mut context.func);
    let pushId = ResolveCheckpointFunction(module, STACK_MAP_PUSH, 3, checkpoints.push)?;
    let popId = ResolveCheckpointFunction(module, STACK_MAP_POP, 1, checkpoints.pop)?;
    let push = module.declare_func_in_func(pushId, &mut context.func);
    let pop = module.declare_func_in_func(popId, &mut context.func);
    let frame = context.func.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        CHECKPOINT_BYTES,
        CHECKPOINT_ALIGNMENT.ilog2() as u8,
    ));

    for (call, roots) in mappedCalls {
        let mut cursor = FuncCursor::new(&mut context.func).at_inst(call);
        cursor.use_srcloc(call);
        let checkpoint = cursor.ins().stack_addr(pointer, frame, 0);
        let stackPointer = cursor.ins().get_stack_pointer(pointer);
        let tableAddress = cursor.ins().symbol_value(pointer, table);
        let publish = cursor.ins().call(push, &[checkpoint, stackPointer, tableAddress]);
        cursor.func.dfg.append_user_stack_map_entries(publish, roots);
        cursor.next_inst();
        cursor.ins().call(pop, &[checkpoint]);
    }

    module.define_function(functionId, context)?;
    let compiled = context
        .compiled_code()
        .ok_or_else(|| ModuleError::Backend(anyhow::anyhow!("compiled function has no machine code")))?;
    let maps = compiled.buffer.user_stack_maps();
    if maps.is_empty() {
        return Err(ModuleError::Backend(anyhow::anyhow!("machine code lost published GC stack maps")));
    }
    let endian = module.isa().endianness();
    let mut bytes = Vec::from(TABLE_MAGIC.as_slice());
    WriteWord(&mut bytes, 0, endian);
    WriteWord(&mut bytes, compiled.buffer.total_size(), endian);
    bytes.resize(FUNCTION_ADDRESS_OFFSET as usize + pointerSize as usize, 0);
    WriteWord(&mut bytes, u32::try_from(maps.len()).map_err(StackMapSizeError)?, endian);
    for (returnPc, frameSize, map) in maps {
        let roots = map.entries().map(|(_, offset)| offset).collect::<Vec<_>>();
        WriteWord(&mut bytes, *returnPc, endian);
        WriteWord(&mut bytes, *frameSize, endian);
        WriteWord(&mut bytes, u32::try_from(roots.len()).map_err(StackMapSizeError)?, endian);
        for offset in roots {
            WriteWord(&mut bytes, offset, endian);
        }
    }
    bytes.resize(bytes.len().next_multiple_of(pointerSize as usize), 0);
    let length = u32::try_from(bytes.len()).map_err(StackMapSizeError)?;
    bytes[TABLE_LENGTH_OFFSET..TABLE_LENGTH_OFFSET + 4].copy_from_slice(&WordBytes(length, endian));

    let mut data = DataDescription::new();
    data.define(bytes.into_boxed_slice());
    data.set_align(u64::from(pointerSize));
    if let Some(section) = section {
        data.set_custom_section(section);
    }
    // ELF uses a local label to prevent preemption of an exported function's
    // address. The Mach-O object writer cannot encode a label symbol. Mach-O
    // uses the definition in this linkage unit under its two-level namespace.
    let function = if module.isa().triple().binary_format.to_string() == "macho" {
        module.declare_func_in_data(functionId, &mut data)
    } else {
        data.import_function(ModuleRelocTarget::FunctionOffset(functionId, 0))
    };
    data.write_function_addr(FUNCTION_ADDRESS_OFFSET, function);
    module.define_data(tableId, &data)
}

fn ResolveCheckpointFunction<M: Module>(
    module: &mut M,
    name: &str,
    arity: usize,
    definition: Option<FuncId>,
) -> ModuleResult<FuncId> {
    if let Some(FuncOrDataId::Func(id)) = module.get_name(name)
        && module.declarations().get_function_decl(id).linkage.is_definable()
        && definition != Some(id)
    {
        return Err(ModuleError::Backend(anyhow::anyhow!("'{name}' belongs to the Beskid runtime")));
    }
    let mut signature = module.make_signature();
    signature.params.resize(arity, AbiParam::new(module.isa().pointer_type()));
    if let Some(id) = definition {
        let declaration = module.declarations().get_function_decl(id);
        if !declaration.linkage.is_definable() || declaration.signature != signature {
            return Err(ModuleError::Backend(anyhow::anyhow!("invalid canonical checkpoint definition '{name}'")));
        }
        return Ok(id);
    }
    module.declare_function(name, Linkage::Import, &signature)
}

fn StackMapSizeError(error: std::num::TryFromIntError) -> ModuleError {
    ModuleError::Backend(anyhow::Error::new(error).context("Beskid GC stack map table exceeds its 32-bit size limit"))
}

fn WriteWord(bytes: &mut Vec<u8>, value: u32, endian: Endianness) {
    bytes.extend(WordBytes(value, endian));
}

fn WordBytes(value: u32, endian: Endianness) -> [u8; 4] {
    match endian {
        Endianness::Little => value.to_le_bytes(),
        Endianness::Big => value.to_be_bytes(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cranelift_jit::{JITBuilder, JITModule};

    #[test]
    fn CanonicalCheckpointUsesLocalDefinitionWithAnInternalName() {
        let builder = JITBuilder::new(cranelift_module::default_libcall_names()).expect("host JIT");
        let mut module = JITModule::new(builder);
        let logicalName = "PushStackMap#syntax_src_Runtime_Bootstrap_Roots_bd";
        let pointer = module.isa().pointer_type();
        let mut signature = module.make_signature();
        signature.params.extend([AbiParam::new(pointer); 3]);
        let function = module.declare_function(logicalName, Linkage::Local, &signature).expect("local checkpoint");
        let definitions = RuntimeCheckpointDefinitions { push: Some(logicalName.to_owned()), pop: None };
        let checkpoints = definitions.Resolve(&HashMap::from([(logicalName.to_owned(), function)]));

        let resolved = ResolveCheckpointFunction(&mut module, STACK_MAP_PUSH, 3, checkpoints.push)
            .expect("resolve the proven definition");
        assert_eq!(resolved, function);
        assert!(module.get_name(STACK_MAP_PUSH).is_none(), "a local definition needs no runtime import");
    }

    #[test]
    fn CanonicalCheckpointRejectsAWrongParameterType() {
        let builder = JITBuilder::new(cranelift_module::default_libcall_names()).expect("host JIT");
        let mut module = JITModule::new(builder);
        let pointer = module.isa().pointer_type();
        let mut signature = module.make_signature();
        signature.params.extend([AbiParam::new(pointer), AbiParam::new(types::I16), AbiParam::new(pointer)]);
        let function = module.declare_function(STACK_MAP_PUSH, Linkage::Local, &signature).expect("local checkpoint");

        let error = ResolveCheckpointFunction(&mut module, STACK_MAP_PUSH, 3, Some(function))
            .expect_err("reject a canonical checkpoint with a mismatched ABI");
        assert!(error.to_string().contains("invalid canonical checkpoint definition"));
    }
}
