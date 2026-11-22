//! Conservative last use for local managed handles after managed CLIF adaptation.

#![allow(non_snake_case)]

use std::collections::HashMap;

use anyhow::{Result, ensure};
use beskid_codegen::CodegenArtifact;
use cranelift_codegen::{
    cursor::{Cursor, FuncCursor},
    ir::{
        AbiParam, Block, ExtFuncData, ExternalName, Function, Inst, InstBuilder, InstructionData, Opcode, Signature,
        Value, types::I32,
    },
};

use crate::browser_heap::{ArrayElement, ArrayOp, HeapLayout, RELEASE_HANDLE, RecordOp};

#[derive(Clone, Copy, PartialEq, Eq)]
enum RootKind {
    Record(usize),
    Array(ArrayElement),
}

#[derive(Clone, Copy)]
enum Helper {
    Allocate { kind: RootKind, arity: usize },
    Access(RootKind),
}

struct Candidate {
    kind: RootKind,
    block: Block,
    lastUse: Inst,
    escapes: bool,
}

/// Release a newly allocated value after its last proven local use. Values
/// passed across calls or branches keep their existing invocation-wide root.
pub fn release_transient_values(artifact: &mut CodegenArtifact, layout: &HeapLayout) -> Result<usize> {
    ensure!(
        !artifact
            .extern_imports
            .iter()
            .chain(&artifact.trusted_extern_imports)
            .any(|import| import.symbol == RELEASE_HANDLE)
            && !artifact.functions.iter().any(|lowered| {
                lowered.name == RELEASE_HANDLE || lowered.function.dfg.ext_funcs.values().any(|external| {
                    matches!(&external.name, ExternalName::TestCase(name) if name.raw() == RELEASE_HANDLE.as_bytes())
                })
            }),
        "managed CLIF uses reserved browser symbol {RELEASE_HANDLE}"
    );

    let mut helpers = HashMap::new();
    for (record, fields) in layout.records.iter().enumerate() {
        let kind = RootKind::Record(record);
        helpers.insert(RecordOp::Alloc.symbol(record), Helper::Allocate { kind, arity: 0 });
        for field in 0..fields.len() {
            helpers.insert(RecordOp::Get(field).symbol(record), Helper::Access(kind));
            helpers.insert(RecordOp::Set(field).symbol(record), Helper::Access(kind));
        }
    }
    for element in layout.arrayElements() {
        let kind = RootKind::Array(element);
        helpers.insert(ArrayOp::Alloc.symbol(element), Helper::Allocate { kind, arity: 1 });
        for access in [ArrayOp::Get, ArrayOp::Set, ArrayOp::Len] {
            helpers.insert(access.symbol(element), Helper::Access(kind));
        }
    }
    Ok(artifact.functions.iter_mut().map(|lowered| releaseInFunction(&mut lowered.function, &helpers)).sum())
}

fn releaseInFunction(function: &mut Function, helpers: &HashMap<String, Helper>) -> usize {
    let mut candidates = HashMap::new();
    for block in function.layout.blocks() {
        for inst in function.layout.block_insts(block) {
            let Some(Helper::Allocate { kind, arity }) = callHelper(function, inst, helpers) else { continue };
            let [handle] = function.dfg.inst_results(inst) else { continue };
            if function.dfg.inst_args(inst).len() == arity && function.dfg.value_type(*handle) == I32 {
                candidates.insert(*handle, Candidate { kind, block, lastUse: inst, escapes: false });
            }
        }
    }
    if candidates.is_empty() {
        return 0;
    }

    for block in function.layout.blocks() {
        for inst in function.layout.block_insts(block) {
            for (argumentIndex, value) in function.dfg.inst_values(inst).enumerate() {
                let resolved = function.dfg.resolve_aliases(value);
                let Some(candidate) = candidates.get_mut(&resolved) else { continue };
                let validUse = argumentIndex == 0
                    && candidate.block == block
                    && (matches!(function.dfg.insts[inst], InstructionData::CondTrap { opcode: Opcode::Trapz, .. })
                        || matches!(callHelper(function, inst, helpers), Some(Helper::Access(kind)) if kind == candidate.kind));
                if validUse {
                    candidate.lastUse = inst;
                } else {
                    candidate.escapes = true;
                }
            }
        }
    }

    let mut releases: Vec<(Inst, Value)> = candidates
        .into_iter()
        .filter_map(|(handle, candidate)| (!candidate.escapes).then_some((candidate.lastUse, handle)))
        .collect();
    if releases.is_empty() {
        return 0;
    }
    releases.sort_unstable();
    let mut signature = Signature::new(function.signature.call_conv);
    signature.params.push(AbiParam::new(I32));
    let signature = function.import_signature(signature);
    let release = function.import_function(ExtFuncData {
        name: ExternalName::testcase(RELEASE_HANDLE),
        signature,
        colocated: false,
        patchable: false,
    });
    let mut cursor = FuncCursor::new(function);
    for &(lastUse, handle) in &releases {
        cursor.goto_inst(lastUse);
        cursor.next_inst();
        cursor.ins().call(release, &[handle]);
    }
    releases.len()
}

fn callHelper(function: &Function, inst: Inst, helpers: &HashMap<String, Helper>) -> Option<Helper> {
    let InstructionData::Call { opcode: Opcode::Call, func_ref, .. } = function.dfg.insts[inst] else { return None };
    let ExternalName::TestCase(name) = &function.dfg.ext_funcs[func_ref].name else { return None };
    helpers.get(std::str::from_utf8(name.raw()).ok()?).copied()
}
