//! Optional qualification evidence from the actual source-to-CLIF artifact.

use anyhow::{Context, Result};
use beskid_abi::runtime_source::{
    CorelibService, canonical_corelib_service_sources, corelib_service_source_descriptor,
};
use beskid_codegen::CodegenArtifact;
use cranelift_codegen::ir::{ExternalName, Function, InstructionData, Opcode};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};

struct Calls<'a> {
    direct: BTreeSet<&'a str>,
    unresolved: bool,
}

fn calls(function: &Function) -> Result<Calls<'_>> {
    let mut direct = BTreeSet::new();
    let mut unresolved = false;
    for block in function.layout.blocks() {
        for inst in function.layout.block_insts(block) {
            let data = &function.dfg.insts[inst];
            match data {
                InstructionData::Call { func_ref, .. } => match &function.dfg.ext_funcs[*func_ref].name {
                    ExternalName::TestCase(name) => {
                        direct.insert(std::str::from_utf8(name.raw())?);
                    }
                    _ => unresolved = true,
                },
                _ if matches!(
                    data.opcode(),
                    Opcode::CallIndirect | Opcode::ReturnCallIndirect | Opcode::TryCallIndirect | Opcode::TryCall
                ) =>
                {
                    unresolved = true;
                }
                _ => {}
            }
        }
    }
    Ok(Calls { direct, unresolved })
}

fn service_key(service: CorelibService) -> (&'static str, &'static str, &'static str) {
    (service.source_path, service.name, service.symbol)
}

fn inventory() -> Vec<Value> {
    let mut services = BTreeMap::new();
    for source in canonical_corelib_service_sources() {
        let Some(descriptor) = corelib_service_source_descriptor(&source.logical_path) else { continue };
        for &service in descriptor.services() {
            services.insert(
                service_key(service),
                json!({
                    "service": service,
                    "package": descriptor.package(),
                    "path": descriptor.relative_path().to_string_lossy().replace('\\', "/"),
                }),
            );
        }
    }
    services.into_values().collect()
}

pub(crate) fn capture(artifact: &CodegenArtifact, exports: &BTreeMap<String, String>) -> Result<Value> {
    let graph = artifact
        .functions
        .iter()
        .map(|lowered| Ok((lowered.name.as_str(), calls(&lowered.function)?)))
        .collect::<Result<HashMap<_, _>>>()?;
    let mut traced_exports = BTreeMap::new();
    let mut declarations = BTreeMap::new();
    for (symbol, export) in exports {
        let declaration = artifact
            .browser_coverage
            .get(symbol)
            .with_context(|| format!("browser qualification lacks the semantic source for export {export}"))?;
        let mut pending = vec![symbol.as_str()];
        let mut visited = BTreeSet::new();
        let mut services = BTreeMap::new();
        let mut edges = Vec::new();
        let mut unresolved = false;
        while let Some(caller) = pending.pop() {
            if !visited.insert(caller) {
                continue;
            }
            let calls = graph.get(caller).context("missing browser qualification call graph entry")?;
            unresolved |= calls.unresolved;
            for &callee in &calls.direct {
                edges.push([caller, callee]);
                if graph.contains_key(callee) {
                    pending.push(callee);
                }
            }
            if let Some(origin) = artifact.browser_coverage.get(caller) {
                declarations.entry(caller).or_insert_with(|| json!({
                    "path": origin.path,
                    "span": origin.span,
                    "signature": artifact.source_signatures.get(caller),
                }));
                for &service in &origin.services {
                    if calls.direct.contains(service.symbol) {
                        services.insert(service_key(service), service);
                    }
                }
            }
        }
        edges.sort_unstable();
        traced_exports.insert(
            export,
            json!({
                "source": { "path": declaration.path, "span": declaration.span },
                "corelibServices": services.into_values().collect::<Vec<_>>(),
                "clifCallEdges": edges,
                "sourceDeclarations": visited.iter().filter(|symbol| declarations.contains_key(**symbol)).collect::<Vec<_>>(),
                "unresolvedCalls": unresolved,
            }),
        );
    }
    Ok(json!({
        "formatVersion": 1,
        "canonicalCorelibServices": inventory(),
        "exports": traced_exports,
        "declarations": declarations,
    }))
}

pub(crate) fn append(bytes: &mut Vec<u8>, evidence: Vec<u8>) {
    let section = wasm_encoder::CustomSection { name: "beskid.coverage".into(), data: evidence.into() };
    wasm_encoder::Section::append_to(&section, bytes);
}

/// Count original CLIF instructions at their original block and instruction IDs.
/// The full printed instruction includes operands, results and types. Insertions
/// cannot count as reuse even when they happen to have the same opcode.
#[derive(Default)]
pub(crate) struct LoweringSnapshot {
    blocks: BTreeSet<cranelift_codegen::ir::Block>,
    instructions: HashMap<(cranelift_codegen::ir::Block, cranelift_codegen::ir::Inst), String>,
}

impl LoweringSnapshot {
    pub fn capture(function: &Function) -> Self {
        let mut snapshot = Self::default();
        for block in function.layout.blocks() {
            snapshot.blocks.insert(block);
            for inst in function.layout.block_insts(block) {
                snapshot.instructions.insert((block, inst), function.dfg.display_inst(inst).to_string());
            }
        }
        snapshot
    }

    pub fn compare(&self, adapted: &Function) -> Value {
        let after = Self::capture(adapted);
        let identical = after.instructions.iter().filter(|(site, text)| self.instructions.get(site) == Some(*text)).count();
        json!({
            "originalInstructions": self.instructions.len(), "adaptedInstructions": after.instructions.len(),
            "identicalInstructions": identical, "originalBlocks": self.blocks.len(),
            "retainedBlocks": self.blocks.intersection(&after.blocks).count(), "adaptedBlocks": after.blocks.len()
        })
    }
}
