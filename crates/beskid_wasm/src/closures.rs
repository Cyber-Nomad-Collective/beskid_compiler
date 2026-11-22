//! Validate and retain source-proven lambda entry points for browser lowering.

use std::collections::{HashMap, HashSet};

use anyhow::{Result, ensure};
use beskid_codegen::CodegenArtifact;
use cranelift_codegen::ir::{ExternalName, GlobalValueData, Opcode};

/// Keep CLIF reachable from a browser entry, including address-taken functions
/// and callbacks reached through compiler-generated static witness tables.
/// Source discovery can also emit unspecialized copies of a specialized callee.
pub(crate) fn retain_reachable(artifact: &mut CodegenArtifact) -> Result<()> {
    let names: HashMap<_, _> = artifact.functions.iter().enumerate()
        .map(|(index, f)| (f.name.as_str(), index)).collect();
    let mut pending = Vec::new();
    for root in artifact.executable_entry.iter().chain(artifact.browser_exports.keys())
        .chain(artifact.spawn_artifacts.iter().map(|spawn| &spawn.trampoline))
        .chain(artifact.witness_tables.iter().flat_map(|table| &table.entries))
        .map(String::as_str).chain(artifact.browser_dynamic.factories())
    {
        pending.push(*names.get(root).ok_or_else(|| anyhow::anyhow!("missing browser entry {root}"))?);
    }
    let mut reachable = HashSet::new();
    while let Some(index) = pending.pop() {
        if !reachable.insert(index) { continue; }
        let function = &artifact.functions[index].function;
        let globals = function.global_values.values().filter_map(|global| match global {
            GlobalValueData::Symbol { name, .. } => Some(name),
            _ => None,
        });
        for symbol in function.dfg.ext_funcs.values().map(|external| &external.name).chain(globals) {
            if let ExternalName::TestCase(name) = symbol
                && let Some(index) = names.get(std::str::from_utf8(name.raw())?)
            {
                pending.push(*index);
            }
        }
    }
    let mut index = 0;
    artifact.functions.retain(|_| {
        let keep = reachable.contains(&index);
        index += 1;
        keep
    });
    Ok(())
}

pub(crate) fn validate_indirect(artifact: &CodegenArtifact) -> Result<()> {
    for lowered in &artifact.functions {
        for block in lowered.function.layout.blocks() {
            for inst in lowered.function.layout.block_insts(block) {
                let data = &lowered.function.dfg.insts[inst];
                let opcode = data.opcode();
                let is_spawn_address = match data {
                    cranelift_codegen::ir::InstructionData::FuncAddr { func_ref, .. } => {
                        match &lowered.function.dfg.ext_funcs[*func_ref].name {
                            ExternalName::TestCase(name) => artifact.spawn_artifacts.iter().any(|spawn| name.raw() == spawn.trampoline.as_bytes()),
                            _ => false,
                        }
                    }
                    _ => false,
                };
                ensure!(is_spawn_address || (artifact.contract_boxes_required && opcode == Opcode::CallIndirect) ||
                    artifact.event_handler_wrapper_required || !artifact.browser_event_layouts.is_empty() || !matches!(opcode, Opcode::CallIndirect | Opcode::ReturnCallIndirect | Opcode::TryCallIndirect | Opcode::FuncAddr),
                    "browser target requires a source-proven closure function value"
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::process::{Command, Stdio};

    use crate::browser_heap::{self, Field, HeapLayout, Scalar};

    #[test]
    fn closure_environment_field_preserves_nullable_reference_identity() {
        let layout = HeapLayout {
            arrays: Default::default(),
            observers: None,
            events: Vec::new(),
            records: vec![vec![Field::AnyRecord, Field::Record(1), Field::Array(browser_heap::ArrayElement::Scalar(Scalar::I32))], vec![Field::Scalar(Scalar::I64)]],
        };
        let bytes = browser_heap::encode(&layout);
        let mut node = Command::new("node")
            .args([
                "--expose-gc",
                "-e",
                r#"
const assert = require("node:assert/strict");
const bytes = require("node:fs").readFileSync(0);
const heap = new WebAssembly.Instance(new WebAssembly.Module(bytes)).exports;
const wrapper = heap.record_0_alloc();
assert.equal(heap.record_0_get_0(wrapper), 0);
assert.equal(heap.record_0_get_1(wrapper), 0);
assert.equal(heap.record_0_get_2(wrapper), 0);
heap.record_0_set_2(wrapper, 0);
const environment = heap.record_1_alloc();
heap.record_0_set_1(wrapper, environment);
assert.equal(heap.record_0_get_1(wrapper), environment);
heap.record_0_set_1(wrapper, 0);
assert.equal(heap.record_0_get_1(wrapper), 0);
heap.record_1_set_0(environment, 73n);
heap.record_0_set_0(wrapper, environment);
global.gc();
const recovered = heap.record_0_get_0(wrapper);
assert.equal(recovered, environment);
assert.equal(heap.record_1_get_0(recovered), 73n);
heap.record_0_set_0(wrapper, 0);
assert.equal(heap.record_0_get_0(wrapper), 0);
"#,
            ])
            .stdin(Stdio::piped())
            .spawn()
            .expect("Node with WebAssembly GC");
        node.stdin.take().expect("Node stdin").write_all(&bytes).expect("write heap module");
        assert!(node.wait().expect("wait for Node").success());
    }
}
