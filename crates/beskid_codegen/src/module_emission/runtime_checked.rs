//! Checked runtime constructors emitted from exact embedded source, sharing the ordinary
//! descriptor pass. These private exports do not grant source callers recovery authority.
use super::contracts::{SyntaxModuleEmissionError, emission_verification};
use super::imports::{ArtifactCallImporter, ArtifactStringInterner};
use super::items::ResolvedSyntaxModuleItem;
use crate::{CodegenContext, CodegenInput, LoweredFunction};
use beskid_isle::DirectCallee;
use beskid_queries::SourceUnitId;
use cranelift_codegen::isa::TargetIsa;
use std::collections::HashMap;

pub(super) fn emit_checked_runtime_constructors(
    input: &CodegenInput<'_>,
    isa: &dyn TargetIsa,
    items: &[ResolvedSyntaxModuleItem],
    symbols: &HashMap<DirectCallee, String>,
    context: &mut CodegenContext,
    functions: &mut Vec<LoweredFunction>,
) -> Result<Vec<String>, SyntaxModuleEmissionError> {
    if input.runtime_intrinsic_capability().is_none() {
        return Ok(Vec::new());
    }
    let mut exports = Vec::new();
    for &beskid_abi::runtime_source::CheckedRuntimeClone { source_path: path, function: name, export } in
        beskid_abi::runtime_source::CHECKED_RUNTIME_CLONES
    {
        let canonical = beskid_abi::runtime_source::canonical_runtime_sources()
            .into_iter()
            .find(|unit| unit.logical_path == path)
            .ok_or_else(|| emission_verification("canonical checked constructor source absent"))?;
        let candidates = items
            .iter()
            .filter(|item| {
                beskid_queries::item_name(input.database(), item.key)
                    .ok()
                    .flatten()
                    .is_some_and(|value| value.as_ref() == name)
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            continue;
        }
        let [entry] = candidates.as_slice() else {
            return Err(emission_verification("canonical checked constructor missing/ambiguous"));
        };
        let source = input
            .typed_program()
            .assembly
            .units
            .iter()
            .find(|unit| SourceUnitId::new(input.database(), unit.path.clone()) == entry.key.unit)
            .ok_or_else(|| emission_verification("checked constructor source owner absent"))?;
        if source.logical_name != canonical.logical_path
            || source.source != canonical.source
            || entry.specialization.is_some()
        {
            return Err(emission_verification("checked constructor is not the exact embedded declaration"));
        }
        let proof = input.checked_effect_closure(isa, entry.key, None).map_err(|failure| {
            emission_verification(format!("checked constructor {name} closure rejected: {}", failure.reason))
        })?;
        let mut checked_symbols = symbols.clone();
        let mut members = proof.members().collect::<Vec<_>>();
        members.sort_by_key(|(callee, _)| format!("{callee:?}"));
        for (ordinal, (callee, _)) in members.iter().enumerate() {
            checked_symbols.insert(
                (*callee).clone(),
                if *callee == proof.entry() { export.into() } else { format!("{export}_closure_{ordinal}") },
            );
        }
        for (callee, member) in members {
            let mut importer = ArtifactCallImporter { symbols: &checked_symbols };
            let mut strings = ArtifactStringInterner { context, pointer_type: isa.pointer_type() };
            let function = crate::checked_effect::emit_artifact_checked_effect_item(
                input,
                isa,
                member,
                &mut strings,
                &mut importer,
            )
            .map_err(emission_verification)?;
            functions.push(LoweredFunction { name: checked_symbols[callee].clone(), function });
        }
        exports.push(export.into());
    }
    Ok(exports)
}
