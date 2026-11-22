//! Browser admission for compiler-generated fiber operations.
use std::collections::HashSet;
use anyhow::{Result, ensure};
use cranelift_codegen::ir::{AbiParam, Signature, types::{I8, I32, I64}};

pub const SPAWN: &str = "fiber_spawn";
pub const JOIN_STATUS: &str = "fiber_join_status";
pub const CANCEL: &str = "fiber_cancel";
pub const DETACH: &str = "fiber_detach";
pub const JOIN_VALUE: &str = "fiber_join_value";
pub const JOIN_DETAIL: &str = "fiber_join_detail";
pub const JOIN_MESSAGE: &str = "fiber_join_message";
pub const JOIN_ERROR_FINISH: &str = "fiber_join_error_finish";
pub const STACK_CHECK: &str = "beskid_rt_v5_scheduler_stack_check";
pub const STACK_OVERFLOW: &str = "beskid_rt_v5_scheduler_stack_overflow_observed";

pub fn admit(name: &str, signature: &Signature, services: &HashSet<String>, has_spawn: bool) -> Result<bool> {
    let (params, result): (&[_], &[_]) = match name {
        SPAWN if has_spawn => (&[I64, I64], &[I64]),
        JOIN_STATUS => (&[I64], &[I32]),
        DETACH => (&[I64], &[]),
        STACK_ENTER | STACK_LEAVE if has_spawn => (&[I64], &[]),
        CANCEL => (&[I64, I64], &[I8]),
        JOIN_VALUE => (&[I64], &[I64]),
        JOIN_DETAIL => (&[I64, I64], &[I64]),
        JOIN_MESSAGE => (&[I64], &[I64]),
        JOIN_ERROR_FINISH => (&[I64], &[I8]),
        STACK_CHECK if has_spawn => (&[I64], &[I8]),
        STACK_OVERFLOW if has_spawn => (&[], &[]),
        _ => return Ok(false),
    };
    ensure!(matches!(name, SPAWN | STACK_CHECK | STACK_OVERFLOW | STACK_ENTER | STACK_LEAVE) || services.contains(name), "fiber service {name} has no source authority");
    ensure!(signature.params == params.iter().copied().map(AbiParam::new).collect::<Vec<_>>() &&
        signature.returns == result.iter().copied().map(AbiParam::new).collect::<Vec<_>>(),
        "fiber service {name} has an invalid native ABI");
    Ok(true)
}

pub const STACK_ENTER: &str = "__beskid_fiber_stack_enter";
pub const STACK_LEAVE: &str = "__beskid_fiber_stack_leave";
#[derive(Debug)]
pub struct Frame {
    marker: i64,
    parameters_and_results: u64,
    slots: u64,
    expected_markers: usize,
}

/// Reserve a deterministic logical frame for each compiled call while a fiber is running.
/// The host tracks counters per fiber and records the exact rejected cumulative reservation.
pub fn instrument(artifact: &mut beskid_codegen::CodegenArtifact) -> Result<Vec<Frame>> {
    use cranelift_codegen::{cursor::{Cursor, FuncCursor}, ir::{ExtFuncData, ExternalName, InstBuilder, Opcode, Signature}};
    if artifact.spawn_artifacts.is_empty() { return Ok(Vec::new()); }
    ensure!(!artifact.extern_imports.iter().any(|external| matches!(external.symbol.as_str(), STACK_ENTER | STACK_LEAVE)), "reserved fiber stack helper was declared by source");
    let mut occupied: HashSet<i64> = artifact.functions.iter().flat_map(|lowered| {
        lowered.function.layout.blocks().flat_map(|block| lowered.function.layout.block_insts(block)).filter_map(|inst| {
            match &lowered.function.dfg.insts[inst] {
                cranelift_codegen::ir::InstructionData::UnaryImm { opcode: Opcode::Iconst, imm } => Some(imm.bits()),
                _ => None,
            }
        })
    }).collect();
    let mut next_marker = i64::MIN;
    let mut frames = Vec::new();
    for lowered in &mut artifact.functions {
        let function = &mut lowered.function;
        ensure!(function.dynamic_stack_slots.is_empty(), "browser fiber has an unbounded dynamic stack frame");
        while occupied.contains(&next_marker) { next_marker = next_marker.checked_add(1).ok_or_else(|| anyhow::anyhow!("fiber frame marker space exhausted"))?; }
        let marker = next_marker;
        occupied.insert(marker);
        next_marker = next_marker.checked_add(1).ok_or_else(|| anyhow::anyhow!("fiber frame marker space exhausted"))?;
        let parameters_and_results = function.signature.params.iter().chain(&function.signature.returns)
            .try_fold(0u64, |size, parameter| size.checked_add(u64::from(parameter.value_type.bytes())))
            .ok_or_else(|| anyhow::anyhow!("fiber signature reservation overflow"))?;
        let mut slots = 0u64;
        for slot in function.sized_stack_slots.values() {
            let alignment = 1u64.checked_shl(u32::from(slot.align_shift)).ok_or_else(|| anyhow::anyhow!("invalid fiber stack alignment"))?;
            slots = slots.checked_add(alignment - 1).map(|size| size & !(alignment - 1)).and_then(|size| size.checked_add(u64::from(slot.size))).ok_or_else(|| anyhow::anyhow!("fiber frame reservation overflow"))?;
        }
        let mut signature = Signature::new(function.signature.call_conv);
        signature.params.push(AbiParam::new(I64));
        let sig = function.import_signature(signature);
        let enter = function.import_function(ExtFuncData { name: ExternalName::testcase(STACK_ENTER), signature: sig, colocated: false, patchable: false });
        let leave = function.import_function(ExtFuncData { name: ExternalName::testcase(STACK_LEAVE), signature: sig, colocated: false, patchable: false });
        let entry = function.layout.entry_block().ok_or_else(|| anyhow::anyhow!("fiber function has no entry"))?;
        let returns: Vec<_> = function.layout.blocks().filter_map(|block| function.layout.last_inst(block)).filter(|inst| matches!(function.dfg.insts[*inst].opcode(), Opcode::Return | Opcode::ReturnCall | Opcode::ReturnCallIndirect)).collect();
        frames.push(Frame { marker, parameters_and_results, slots, expected_markers: returns.len() + 1 });
        let mut cursor = FuncCursor::new(function);
        cursor.goto_first_insertion_point(entry);
        let amount = cursor.ins().iconst(I64, marker);
        cursor.ins().call(enter, &[amount]);
        for inst in returns {
            cursor.goto_inst(inst);
            let amount = cursor.ins().iconst(I64, marker);
            cursor.ins().call(leave, &[amount]);
        }
    }
    Ok(frames)
}

/// Measure the actual emitted Wasm locals and validated operand peak, then replace
/// the reserved ten-byte marker immediates without changing section offsets.
pub fn finalize_frames(bytes: &mut [u8], frames: &[Frame]) -> Result<()> {
    use wasmparser::{FuncValidatorAllocations, Operator, Parser, ValidPayload, Validator, WasmFeatures};
    if frames.is_empty() { return Ok(()); }
    let mut validator = Validator::new_with_features(WasmFeatures::all());
    let mut bodies = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        let payload = payload?;
        if let ValidPayload::Func(function, body) = validator.payload(&payload)? { bodies.push((function, body)); }
    }
    let markers: std::collections::HashMap<i64, usize> = frames.iter().enumerate().map(|(index, frame)| (frame.marker, index)).collect();
    ensure!(markers.len() == frames.len(), "duplicate fiber frame markers");
    let mut seen = HashSet::new();
    let mut patches: Vec<(usize, u64)> = Vec::new();
    let mut allocs = FuncValidatorAllocations::default();
    for (function, body) in bodies {
        let mut local_bytes = 0u64;
        for local in body.get_locals_reader()? {
            let (count, ty) = local?;
            let width = match ty { wasmparser::ValType::I32 | wasmparser::ValType::F32 => 4, wasmparser::ValType::I64 | wasmparser::ValType::F64 | wasmparser::ValType::Ref(_) => 8, wasmparser::ValType::V128 => 16 };
            local_bytes = local_bytes.checked_add(u64::from(count) * width).ok_or_else(|| anyhow::anyhow!("fiber local reservation overflow"))?;
        }
        let mut function = function.into_validator(allocs);
        let mut reader = body.get_binary_reader();
        function.read_locals(&mut reader)?;
        let mut operators = body.get_operators_reader()?;
        let mut peak = 0u64;
        let mut marker_offsets = Vec::new();
        let mut frame_index = None;
        while !operators.eof() {
            let start = operators.original_position();
            let op = operators.read()?;
            let end = operators.original_position();
            function.op(start, &op)?;
            peak = peak.max(u64::from(function.operand_stack_height()));
            if let Operator::I64Const { value } = op {
                if let Some(&index) = markers.get(&value) {
                    ensure!(frame_index.is_none_or(|previous| previous == index), "multiple fiber frame markers in one Wasm body");
                    frame_index = Some(index);
                    ensure!(end - start == 11, "fiber frame marker has unexpected Wasm encoding");
                    marker_offsets.push(start + 1);
                }
            }
        }
        let Some(index) = frame_index else { allocs = function.into_allocations(); continue; };
        ensure!(seen.insert(index), "fiber frame marker appears in multiple Wasm bodies");
        let frame = &frames[index];
        ensure!(marker_offsets.len() == frame.expected_markers, "fiber frame marker is missing or collides with source constants");
        let reserve = 16u64.checked_add(frame.parameters_and_results).and_then(|v| v.checked_add(frame.slots))
            .and_then(|v| v.checked_add(local_bytes)).and_then(|v| peak.checked_mul(16).and_then(|peak| v.checked_add(peak)))
            .ok_or_else(|| anyhow::anyhow!("fiber frame reservation overflow"))?;
        ensure!(reserve <= i64::MAX as u64, "fiber frame reservation exceeds ABI");
        patches.extend(marker_offsets.into_iter().map(|offset| (offset, reserve)));
        allocs = function.into_allocations();
    }
    ensure!(seen.len() == frames.len(), "fiber frame marker is missing from emitted Wasm");
    for (offset, mut reserve) in patches {
        for index in 0..9 { bytes[offset + index] = (reserve as u8 & 0x7f) | 0x80; reserve >>= 7; }
        bytes[offset + 9] = reserve as u8;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Frame, finalize_frames};
    use wasm_encoder::{CodeSection, Function, FunctionSection, Instruction, Module, TypeSection, ValType};
    use wasmparser::{Operator, Parser, Payload};

    #[test]
    fn reservations_measure_emitted_locals_and_operand_peak() {
        let mut types = TypeSection::new();
        types.ty().function([], []);
        let mut functions = FunctionSection::new();
        functions.function(0);
        let mut body = Function::new([(2, ValType::I64)]);
        body.instruction(&Instruction::I64Const(i64::MIN));
        body.instruction(&Instruction::Drop);
        body.instruction(&Instruction::End);
        let mut code = CodeSection::new();
        code.function(&body);
        let mut module = Module::new();
        module.section(&types).section(&functions).section(&code);
        let mut bytes = module.finish();
        let frames = [Frame { marker: i64::MIN, parameters_and_results: 0, slots: 0, expected_markers: 1 }];
        finalize_frames(&mut bytes, &frames).expect("measure frame");
        let mut observed = None;
        for payload in Parser::new(0).parse_all(&bytes) {
            if let Payload::CodeSectionEntry(body) = payload.expect("parse") {
                let mut operators = body.get_operators_reader().expect("operators");
                if let Operator::I64Const { value } = operators.read().expect("constant") { observed = Some(value); }
            }
        }
        assert_eq!(observed, Some(48)); // header 16 + two i64 locals 16 + one operand 16
    }
}
