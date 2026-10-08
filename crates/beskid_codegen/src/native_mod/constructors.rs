//! Source-body constructor bindings, issued from canonical syntax and resolved parameter slots.
use super::{adapter_plan::*, transport_layout::TransportLayouts};
use crate::{CodegenInput, SyntaxModuleItem};
use anyhow::{Context, Result, bail};
use beskid_queries::{
    AstNodeKey, NativeModTransportBody, NativeModTransportType, aggregate_literal_declaration,
    aggregate_literal_field_values, child_nodes, enum_constructor, local_slot, native_mod_transport_nominal,
    native_mod_transport_signature, resolved_local,
};

pub(super) fn issue_constructor(
    input: &CodegenInput<'_>,
    item: &SyntaxModuleItem,
    layouts: &mut TransportLayouts,
) -> Result<NativeConstructor> {
    super::authority::require_sdk_package(input, item.key)?;
    let signature = native_mod_transport_signature(input.database(), item.key)?
        .context("source constructor signature unavailable")?;
    let result_type = layouts.issue(input, item.key, signature.result())?;
    let parameters =
        signature.parameters().iter().map(|ty| layouts.issue(input, item.key, ty)).collect::<Result<Vec<_>>>()?;
    let NativeModTransportType::Nominal { declaration, .. } = signature.result() else {
        bail!("source constructor result is not a nominal");
    };
    let nominal = native_mod_transport_nominal(input.database(), item.key, signature.result())?
        .context("source constructor result projection unavailable")?;
    let mut pending = vec![item.key];
    let mut selected = None;
    let mut visited = std::collections::HashSet::new();
    while let Some(key) = pending.pop() {
        if !visited.insert(key) {
            continue;
        }
        if visited.len() > 100000 {
            bail!("source constructor traversal budget exceeded");
        }
        if aggregate_literal_declaration(input.database(), key)? == Some(*declaration) {
            let fields = aggregate_literal_field_values(input.database(), key)?
                .context("source constructor fields unavailable")?;
            let NativeModTransportBody::Record(projected) = nominal.body() else {
                bail!("record constructor has enum result");
            };
            let mut bindings = vec![None; parameters.len()];
            for (ordinal, field) in projected.iter().enumerate() {
                let value = fields
                    .iter()
                    .find(|(name, _)| name.as_ref() == field.name())
                    .context("source constructor omits declared field")?
                    .1;
                bind_parameter(input, item.key, value, ordinal, &mut bindings)?;
            }
            let bindings = bindings
                .into_iter()
                .map(|binding| binding.context("source constructor parameter lacks field binding"))
                .collect::<Result<Vec<_>>>()?;
            if selected.replace((None, bindings)).is_some() {
                bail!("source constructor has ambiguous allocation bodies");
            }
        }
        if let Some(constructor) = enum_constructor(input.database(), key)? {
            if constructor.declaration == *declaration {
                let NativeModTransportBody::Enum(variants) = nominal.body() else {
                    bail!("enum constructor has record result");
                };
                let variant = variants
                    .get(constructor.variant_index as usize)
                    .context("source enum constructor tag is absent")?;
                if variant.fields().len() != constructor.payloads.len() {
                    bail!("source enum constructor payload arity differs");
                }
                let mut bindings = vec![None; parameters.len()];
                for (ordinal, value) in constructor.payloads.iter().enumerate() {
                    bind_parameter(input, item.key, *value, ordinal, &mut bindings)?;
                }
                let bindings = bindings
                    .into_iter()
                    .map(|binding| binding.context("source constructor parameter lacks payload binding"))
                    .collect::<Result<Vec<_>>>()?;
                if selected.replace((Some(constructor.variant_index), bindings)).is_some() {
                    bail!("source constructor has ambiguous enum bodies");
                }
            }
        }
        if let Some(children) = child_nodes(input.database(), key)? {
            pending.extend(children.iter().copied());
        }
    }
    let (variant, bindings) = selected.context("source constructor does not allocate its declared result")?;
    Ok(NativeConstructor {
        symbol: crate::internal_link_symbol(&item.symbol),
        result_type,
        variant,
        parameters,
        bindings,
    })
}
fn bind_parameter(
    input: &CodegenInput<'_>,
    owner: AstNodeKey,
    value: AstNodeKey,
    field: usize,
    bindings: &mut [Option<NativeConstructorArgument>],
) -> Result<()> {
    // Field values are registered as `Expression` wrapper nodes; the exact parameter path or
    // single-element array literal is the payload beneath them.
    let value = beskid_queries::native_mod_expression_payload(input.database(), value)?
        .context("source constructor field value is not a current expression")?;
    let (reference, binding_kind) = if resolved_local(input.database(), value)?.is_some() {
        (value, NativeConstructorArgument::Field(u32::try_from(field)?))
    } else {
        let element = beskid_queries::native_mod_single_array_element(input.database(), value)?
            .context("source constructor requires exact single-element array field")?;
        let mut descendants = vec![element];
        let mut matches = Vec::new();
        while let Some(child) = descendants.pop() {
            if resolved_local(input.database(), child)?.is_some() {
                matches.push(child);
            } else if let Some(children) = child_nodes(input.database(), child)? {
                descendants.extend(children.iter().copied());
            }
        }
        if matches.len() != 1 {
            bail!("source constructor array field lacks one exact parameter");
        }
        (matches[0], NativeConstructorArgument::ArrayElement { field: u32::try_from(field)?, index: 0 })
    };
    let local = resolved_local(input.database(), reference)?
        .context("source constructor field is not an exact parameter reference")?;
    let slot =
        local_slot(input.database(), local.declaration)?.context("source constructor parameter slot unavailable")?;
    if slot.owner != owner {
        bail!("source constructor field captures a foreign parameter");
    }
    let binding =
        bindings.get_mut(slot.index as usize).context("source constructor field references a nonparameter local")?;
    if binding.replace(binding_kind).is_some() {
        bail!("source constructor duplicates a parameter");
    }
    Ok(())
}

pub(super) fn is_callback_value_constructor(input:&CodegenInput<'_>,item:&SyntaxModuleItem)->Result<bool> {
    let unit=input.typed_program().assembly.units.iter().find(|unit|beskid_queries::SourceUnitId::new(input.database(),unit.path.clone())==item.key.unit).context("semantic constructor unit absent")?;
    let Some(proof)=input.typed_program().assembly.package_identities().for_source(&unit.path) else {return Ok(false)};
    let relative=proof.relative_source_path(&unit.path);
    let canonical=match relative.as_deref(){Some("Beskid/Compiler/Semantic.bd")=>"src/Beskid/Compiler/Semantic.bd",Some("Beskid/Compiler/Query.bd")=>"src/Beskid/Compiler/Query.bd",_=>return Ok(false)};
    super::authority::require_sdk_package(input,item.key)?;
    beskid_abi::sdk_source::verify_canonical_sdk_source(canonical,unit.source.as_bytes())?;
    let Some(signature)=native_mod_transport_signature(input.database(),item.key)? else {return Ok(false)};
    let NativeModTransportType::Nominal{declaration,..}=signature.result() else {return Ok(false)};
    let mut pending=vec![item.key];let mut visited=std::collections::HashSet::new();
    while let Some(key)=pending.pop(){if !visited.insert(key){continue;}if visited.len()>100000{bail!("semantic constructor traversal budget exceeded");}
        if aggregate_literal_declaration(input.database(),key)?==Some(*declaration) {return Ok(true)};
        if enum_constructor(input.database(),key)?.is_some_and(|constructor|constructor.declaration==*declaration){return Ok(true)};
        if let Some(children)=child_nodes(input.database(),key)?{pending.extend(children.iter().copied());}
    }
    Ok(false)
}
