//! Witness tables of an artifact (runtime contract dispatch, v0.6.7): every contract box a lowered
//! item or a composition activation makes names one table, whose entries are the emitted symbols
//! of the implementation's methods in slot order, or unboxing thunks for a primitive receiver.

use std::collections::{BTreeMap, HashSet};

use beskid_queries::{AstNodeKey, ContractBoxReceiver, ContractWitnessTable, GenericSpecializationInstance, SemanticTypeId};
use cranelift_codegen::isa::TargetIsa;

use super::contracts::{SyntaxModuleEmissionError, emission_verification};
use super::items::ResolvedSyntaxModuleItem;
use super::trace::format_declaration_for_trace;
use crate::CodegenInput;
use crate::contract_static::{WitnessTablePlan, WitnessThunkPlan};
use crate::isle_adapter::mappings::specialization_identity;

/// The witness tables and thunks of one artifact.
#[derive(Debug, Default)]
pub(super) struct ContractWitnesses {
    pub tables: Vec<WitnessTablePlan>,
    pub thunks: Vec<WitnessThunkPlan>,
    pub boxes_required: bool,
}

/// Collect every witness table the emitted `items` and the composition plan box with.
pub(super) fn collect_contract_witnesses(
    input: &CodegenInput<'_>,
    items: &[ResolvedSyntaxModuleItem],
    isa: Option<&dyn TargetIsa>,
) -> Result<ContractWitnesses, SyntaxModuleEmissionError> {
    let db = input.database();
    let mut tables = BTreeMap::<String, ContractWitnessTable>::new();
    for item in items {
        let mut visited = HashSet::new();
        let mut nodes = Vec::new();
        super::data::collect_ast_nodes(db, item.key, &mut visited, &mut nodes);
        for key in nodes {
            if let Some(coercion) = beskid_queries::contract_box_coercion(db, key, item.specialization.as_ref())
                .map_err(|error| emission_verification(error.to_string()))?
            {
                tables.entry(coercion.witness.symbol.to_string()).or_insert(coercion.witness);
            }
        }
    }
    for (field, declaration, contract) in composition_contract_injections(input) {
        if let Some(table) = beskid_queries::contract_witness_table_for_declaration(db, field, declaration, contract)
            .map_err(|error| emission_verification(error.to_string()))?
        {
            tables.entry(table.symbol.to_string()).or_insert(table);
        }
    }
    let mut witnesses = ContractWitnesses { boxes_required: !tables.is_empty(), ..Default::default() };
    for (symbol, table) in tables {
        let instances = beskid_queries::contract_witness_method_instances(db, &table)
            .map_err(|error| emission_verification(error.to_string()))?
            .unwrap_or_default();
        let mut entries = Vec::with_capacity(instances.len());
        for (slot, instance) in instances.iter().enumerate() {
            let target = emitted_symbol(items, instance).ok_or_else(|| {
                emission_verification(format!(
                    "witness table `{symbol}` names an implementation method that was not emitted: {}",
                    format_declaration_for_trace(db, instance.declaration)
                ))
            })?;
            match table.receiver {
                ContractBoxReceiver::Reference | ContractBoxReceiver::Scalar(SemanticTypeId::STRING) => {
                    entries.push(target);
                }
                ContractBoxReceiver::Scalar(receiver) => {
                    isa.ok_or_else(|| emission_verification("witness thunks need a target ISA"))?;
                    let thunk = format!("{symbol}__thunk{slot}");
                    witnesses.thunks.push(WitnessThunkPlan {
                        symbol: thunk.clone(),
                        target_symbol: target,
                        receiver,
                        target_signature: instance.signature.clone(),
                    });
                    entries.push(thunk);
                }
            }
        }
        witnesses.tables.push(WitnessTablePlan { symbol, entries });
    }
    Ok(witnesses)
}

/// The exact instances every composition activation boxes into contract-typed injected fields,
/// so the implementation methods join the emission worklist before any table names them.
pub(super) fn composition_witness_instances(
    input: &CodegenInput<'_>,
) -> Result<Vec<GenericSpecializationInstance>, SyntaxModuleEmissionError> {
    let db = input.database();
    let mut instances = Vec::new();
    for (field, declaration, contract) in composition_contract_injections(input) {
        if let Some(table) = beskid_queries::contract_witness_table_for_declaration(db, field, declaration, contract)
            .map_err(|error| emission_verification(error.to_string()))?
        {
            instances.extend(
                beskid_queries::contract_witness_method_instances(db, &table)
                    .map_err(|error| emission_verification(error.to_string()))?
                    .unwrap_or_default()
                    .iter()
                    .cloned(),
            );
        }
    }
    Ok(instances)
}

/// `(field, implementation declaration, contract)` of every contract-typed injection the frozen
/// composition plan activates.
fn composition_contract_injections(input: &CodegenInput<'_>) -> Vec<(AstNodeKey, AstNodeKey, AstNodeKey)> {
    let db = input.database();
    let Some((plan, snapshot)) = input.composition_authority() else {
        return Vec::new();
    };
    let registration_declaration = |slot: u32| -> Option<AstNodeKey> {
        let activation = plan.activation.iter().find(|entry| entry.slot.0 == slot)?;
        let registration = snapshot.registrations.iter().find(|entry| entry.id == activation.registration_id)?;
        let source = input.composition_unit_key(&registration.unit, registration.source_node_id);
        let fact = beskid_queries::composition_registration(db, source).ok().flatten()?;
        (fact.site == source && fact.implementation.as_ref() == registration.implementation).then_some(fact.declaration)
    };
    let mut injections = Vec::new();
    let singulars = plan.singulars.iter().map(|entry| {
        (input.composition_unit_key(&entry.field_unit, entry.field_node_id), vec![entry.target_slot.0])
    });
    let plurals = plan.plurals.iter().map(|entry| {
        (
            input.composition_unit_key(&entry.field_unit, entry.field_node_id),
            entry.target_slots.iter().map(|slot| slot.0).collect::<Vec<_>>(),
        )
    });
    for (field, slots) in singulars.chain(plurals) {
        let Ok(Some(contract)) = beskid_queries::injected_field_contract(db, field) else {
            continue;
        };
        for slot in slots {
            if let Some(declaration) = registration_declaration(slot) {
                injections.push((field, declaration, contract));
            }
        }
    }
    injections
}

/// The emitted symbol of one implementation method instance among `items`.
fn emitted_symbol(items: &[ResolvedSyntaxModuleItem], instance: &GenericSpecializationInstance) -> Option<String> {
    let callee = if instance.substitutions.is_empty() && instance.contract_witnesses.is_empty() {
        beskid_isle::DirectCallee::item(instance.declaration)
    } else {
        beskid_isle::DirectCallee::specialized_item(instance.declaration, specialization_identity(instance))
    };
    items.iter().find(|item| item.callee == callee).map(|item| item.symbol.clone())
}
