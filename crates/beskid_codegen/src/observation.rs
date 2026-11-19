use std::borrow::Cow;
use std::collections::hash_map::Entry;
use std::collections::{BTreeSet, HashMap, HashSet};

use beskid_queries::{
    AggregateSpecialization, AstNodeKey, Db, GenericSpecializationInstance, IndexedNodeKind, ObservedPropertyInstance,
    PropertyReference, SemanticError, SemanticQueryResult,
};

#[derive(Default)]
pub(crate) struct ObservedProperties {
    owners: HashMap<AstNodeKey, ObservedOwner>,
    backings: HashSet<AstNodeKey>,
    dependencies: HashMap<AstNodeKey, Vec<AstNodeKey>>,
}

#[derive(Default)]
struct ObservedOwner {
    contract: HashSet<AstNodeKey>,
    direct: HashMap<Option<AggregateSpecialization>, HashSet<AstNodeKey>>,
}

impl ObservedProperties {
    pub(crate) fn classify_item(
        &mut self,
        db: &dyn Db,
        item: AstNodeKey,
        enclosing: Option<&GenericSpecializationInstance>,
        mut trace: Option<&mut BTreeSet<String>>,
    ) -> Result<(), SemanticError> {
        for instance in beskid_queries::observed_property_instances(db, item, enclosing)?.into_iter().flatten() {
            let owner = self.owners.entry(instance.owner).or_default();
            let properties = if instance.through_contract {
                &mut owner.contract
            } else {
                owner.direct.entry(instance.specialization.clone()).or_default()
            };
            if !properties.insert(instance.property) {
                continue;
            }
            if beskid_queries::node_kind(db, instance.property)? == Some(IndexedNodeKind::MethodDefinition) {
                let dependencies = match self.dependencies.entry(instance.property) {
                    Entry::Occupied(entry) => entry.into_mut(),
                    Entry::Vacant(entry) => {
                        let dependencies = beskid_queries::observation_stored_dependencies(db, instance.property)?
                            .ok_or_else(|| SemanticError::unavailable("observe.derived_dependencies"))?;
                        self.backings.extend(dependencies.iter().copied());
                        entry.insert(dependencies)
                    }
                };
                if let Some(trace) = trace.as_deref_mut() {
                    trace_classification(db, &instance, dependencies, trace);
                }
            } else {
                self.backings.insert(instance.property);
                if let Some(trace) = trace.as_deref_mut() {
                    trace_classification(db, &instance, &[], trace);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.owners.is_empty()
    }

    pub(crate) fn observes_backing(&self, property: AstNodeKey) -> bool {
        self.backings.contains(&property)
    }

    pub(crate) fn has_slot(&self, owner: AstNodeKey, specialization: &Option<AggregateSpecialization>) -> bool {
        self.owners
            .get(&owner)
            .is_some_and(|owner| !owner.contract.is_empty() || owner.direct.contains_key(specialization))
    }

    pub(crate) fn contract_properties(&self, owner: AstNodeKey) -> Option<&HashSet<AstNodeKey>> {
        self.owners.get(&owner).map(|owner| &owner.contract).filter(|properties| !properties.is_empty())
    }

    pub(crate) fn properties(
        &self,
        owner: AstNodeKey,
        specialization: &Option<AggregateSpecialization>,
    ) -> Option<Cow<'_, HashSet<AstNodeKey>>> {
        let owner = self.owners.get(&owner)?;
        match (owner.direct.get(specialization), owner.contract.is_empty()) {
            (Some(direct), true) => Some(Cow::Borrowed(direct)),
            (None, false) => Some(Cow::Borrowed(&owner.contract)),
            (Some(direct), false) => Some(Cow::Owned(direct.union(&owner.contract).copied().collect())),
            (None, true) => None,
        }
    }

    pub(crate) fn dependent_getters(&self, observed: &HashSet<AstNodeKey>, backing: AstNodeKey) -> Vec<AstNodeKey> {
        let mut getters = observed
            .iter()
            .copied()
            .filter(|getter| self.dependencies.get(getter).is_some_and(|dependencies| dependencies.contains(&backing)))
            .collect::<Vec<_>>();
        getters.sort_by_key(|key| key.node.0);
        getters
    }
}

fn trace_classification(
    db: &dyn Db,
    instance: &ObservedPropertyInstance,
    dependencies: &[AstNodeKey],
    records: &mut BTreeSet<String>,
) {
    let owner =
        beskid_queries::registered_declaration_name(db, instance.owner).unwrap_or_else(|| "<unavailable>".to_owned());
    let property_name = |key| {
        beskid_queries::observation_property_name(db, key).ok().flatten().unwrap_or_else(|| "<unavailable>".into())
    };
    let property = property_name(instance.property);
    let applied = if instance.through_contract {
        format!("{owner}<any>")
    } else if let Some(specialization) = &instance.specialization {
        format!("{owner}<{}>", specialization.arguments_display())
    } else {
        owner.clone()
    };
    let reason = if instance.through_contract { "contract" } else { "direct" };
    records.insert(format!(
        "event=observe.classification owner={owner} specialization={applied:?} property={property} reason={reason}"
    ));
    for &dependency in dependencies {
        let name = property_name(dependency);
        records.insert(format!("event=observe.classification owner={owner} specialization={applied:?} property={name} reason=derived_dependency derived={property}"));
    }
}

/// Resolve the actual field write. A custom setter invocation reaches its backing write inside
/// the setter body, so the outer assignment must not acquire a second notification plan.
pub(crate) fn backing_property(
    db: &dyn Db,
    assignment: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<(AstNodeKey, PropertyReference)> {
    let Some(mut target) = beskid_queries::child_nodes(db, assignment)?.and_then(|children| children.first().copied())
    else {
        return Ok(None);
    };
    while beskid_queries::node_kind(db, target)? == Some(IndexedNodeKind::Expression) {
        let Some(child) = beskid_queries::child_nodes(db, target)?.and_then(|children| children.first().copied())
        else {
            return Ok(None);
        };
        target = child;
    }
    let Some(reference) = beskid_queries::property_reference_in_environment(db, target, enclosing)? else {
        return Ok(None);
    };
    if !reference.is_backing_field
        && beskid_queries::property_setter_in_environment(db, assignment, enclosing)?.is_some()
    {
        return Ok(None);
    }
    Ok(Some((target, reference)))
}
