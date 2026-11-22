//! Retained source identities for compiler-generated dependency injection allocations.

use std::{collections::HashMap, sync::Arc};

use beskid_isle::DirectCallee;
use beskid_queries::{SemanticTypeId, SourceNominalType, SourceSignatureType};

use super::contracts::{SyntaxModuleEmissionError, emission_verification};
use super::data::AggregateSourceTypes;
use crate::CodegenInput;
use crate::aggregate_static::{AggregateStaticField, AggregateStaticPlan};

pub(super) fn disposal_hooks(
    input: &CodegenInput<'_>,
    symbols: &HashMap<DirectCallee, String>,
) -> Result<Option<Vec<String>>, SyntaxModuleEmissionError> {
    let Some((plan, _)) = input.composition_authority() else { return Ok(None) };
    plan.disposal_hooks.iter().map(|hook| {
        let key = input.composition_unit_key(&hook.unit, hook.function_node_id);
        symbols.get(&DirectCallee::item(key)).cloned()
            .ok_or_else(|| emission_verification("composition disposal hook has no emitted symbol"))
    }).collect::<Result<_, _>>().map(Some)
}

pub(super) fn retain_registration(
    input: &CodegenInput<'_>,
    plan: &mut AggregateStaticPlan,
    sources: &mut AggregateSourceTypes,
) -> Result<(), SyntaxModuleEmissionError> {
    let db = input.database();
    let registration = beskid_queries::composition_registration(db, plan.literal)
        .map_err(|error| emission_verification(error.to_string()))?
        .ok_or_else(|| emission_verification("composition registration source unavailable"))?;
    let nominal = SourceNominalType { declaration: registration.declaration, arguments: Arc::from([]) };
    let layout = input.aggregate_object_layout(registration.declaration)
        .ok_or_else(|| emission_verification("composition registration layout unavailable"))?;
    let mut source_fields = beskid_queries::source_record_fields(db, &nominal)
        .map_err(|error| emission_verification(error.to_string()))?
        .ok_or_else(|| emission_verification("composition registration field types unavailable"))?
        .to_vec();
    let mut fields = plan.fields.to_vec();
    for &(field, offset) in layout.injected_fields.iter() {
        let ty = beskid_queries::source_injected_field_type(db, field)
            .map_err(|error| emission_verification(error.to_string()))?
            .ok_or_else(|| emission_verification("composition injection type unavailable"))?;
        if !matches!(ty, SourceSignatureType::Nominal(_) | SourceSignatureType::Array(_)) {
            return Err(emission_verification("composition injection has no managed source type"));
        }
        source_fields.push(ty);
        fields.push(AggregateStaticField { abi_type: SemanticTypeId::POINTER, field_offset: offset });
    }
    plan.fields = fields.into();
    sources.fields.insert(nominal.clone(), source_fields.into());
    sources.declarations.insert(plan.allocation_request_symbol.clone(), nominal);
    Ok(())
}
