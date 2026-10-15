//! Exact current SDK conformance selection for executable native adapters.
use crate::CodegenInput;
use anyhow::{Result, bail};
use beskid_queries::{
    AppliedContractIdentity, AstNodeKey, type_applied_contract_implementation, type_contract_applications_of,
    type_contract_declarations,
};

use beskid_queries::NativeModContractFamily as ContractFamily;

/// A witness retains its applied nominal arguments and exact signature/method keys.
/// Physical pointer equivalence never authorizes an implementation.
pub(super) struct ContractWitness {
    pub family: ContractFamily,
    pub owner: AstNodeKey,
    pub application: AppliedContractIdentity,
    pub methods: Vec<(AstNodeKey, AstNodeKey)>,
}

fn canonical_family(input: &CodegenInput<'_>, declaration: AstNodeKey) -> Result<Option<ContractFamily>> {
    let db = input.database();
    let Some(declaration_witness) = beskid_queries::native_mod_contract_declaration(db, declaration)? else {
        return Ok(None);
    };
    let family = declaration_witness.family();
    super::authority::require_sdk_package(input, declaration)?;
    Ok(Some(family))
}

/// Admit only conformances whose contract declaration is a canonical SDK contract. The family is
/// decided from the declaration identity alone, before any applied argument is interpreted, so a
/// dependency type such as `ArrayIterator<T> : Iterator<T>` is never evaluated here.
pub(super) fn select_contract_witnesses(input: &CodegenInput<'_>, owner: AstNodeKey) -> Result<Vec<ContractWitness>> {
    let db = input.database();
    let declarations = type_contract_declarations(db, owner)?
        .ok_or_else(|| anyhow::anyhow!("native Mod owner is not a current registered concrete type"))?;
    let mut witnesses = Vec::new();
    for declaration in declarations.iter().copied() {
        let Some(family) = canonical_family(input, declaration)? else { continue };
        if witnesses.iter().any(|witness: &ContractWitness| witness.family == family) {
            bail!("native Mod type has ambiguous implementations of {}", family.canonical_name());
        }
        let applications = type_contract_applications_of(db, owner, declaration)?
            .ok_or_else(|| anyhow::anyhow!("native Mod owner is not a current registered concrete type"))?;
        let [application] = applications.as_ref() else {
            bail!("native Mod type has ambiguous implementations of {}", family.canonical_name());
        };
        let methods = type_applied_contract_implementation(db, owner, application)?
            .ok_or_else(|| anyhow::anyhow!("native Mod contract lacks exact current implementation witnesses"))?;
        if methods.is_empty() {
            bail!("native Mod contract has no executable methods");
        }
        witnesses.push(ContractWitness { family, owner, application: application.clone(), methods: methods.to_vec() });
    }
    Ok(witnesses)
}

/// Exact applied factory selection. Source receiver construction is planned separately;
/// this witness does not grant authority to pass a null or uninitialized receiver.
pub(super) fn select_instance_factory(
    input: &CodegenInput<'_>,
    receiver: AstNodeKey,
    candidates: &[AstNodeKey],
) -> Result<ContractWitness> {
    let mut selected = None;
    for candidate in candidates {
        for witness in select_contract_witnesses(input, *candidate)? {
            if witness.family != ContractFamily::Factory {
                continue;
            }
            if !beskid_queries::applied_contract_argument_is_type(input.database(), &witness.application, 0, receiver)?
            {
                continue;
            }
            if selected.replace(witness).is_some() {
                bail!("native Mod receiver has ambiguous exact source instance factories");
            }
        }
    }
    selected.ok_or_else(|| anyhow::anyhow!("native Mod receiver has no exact applied source instance factory"))
}
