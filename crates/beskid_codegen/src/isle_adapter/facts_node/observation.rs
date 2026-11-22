use super::super::*;
use beskid_isle::{ObservationEquality, ObservationPlan, ObservedDerived, ObservedProperty, ObservedStore};

impl SyntaxNodeFacts<'_> {
    pub(super) fn observation_impl(&self, key: AstNodeKey) -> Option<ObservationPlan> {
        let children = self.query(child_nodes(self.db, key))?;
        let [target, callback] = children.as_ref() else { return None };
        let target = self.unwrap_transparent(*target)?;
        let enclosing = self.current_item_specialization();
        let property = self.query(beskid_queries::observation_target_in_environment(self.db, key, enclosing))?;
        let (owner, layout) = self.query(beskid_queries::observation_receiver_layout(
            self.db, target, property.receiver, enclosing,
        ))?;
        let layout = self.input.aggregate_object_layout_from_fact(owner, &layout)?;
        Some(ObservationPlan {
            receiver: property.receiver,
            callback: self.unwrap_transparent(*callback)?,
            slot_offset: layout.observer_slot?,
            property_id: u64::from(property.declaration.node.0),
        })
    }

    pub(super) fn observed_store_impl(&self, assignment: AstNodeKey) -> Result<Option<ObservedStore>, ()> {
        if self.input.observed_properties.is_empty() {
            return Ok(None);
        }
        let enclosing = self.current_item_specialization();
        let Some((target, reference)) =
            crate::observation::backing_property(self.db, assignment, enclosing).map_err(|_| ())?
        else {
            return Ok(None);
        };
        if !self.input.observed_properties.observes_backing(reference.declaration) {
            return Ok(None);
        }
        let access = self.aggregate_field_access_in_context(target).ok_or(())?;
        let (owner, receiver_layout) = self.query(beskid_queries::observation_receiver_layout(
            self.db, target, reference.receiver, enclosing,
        )).ok_or(())?;
        if access.declaration != owner {
            return Err(());
        }
        let Some(observed) = self.input.observed_properties.properties(owner, &receiver_layout.specialization) else {
            return Ok(None);
        };
        let dependent = self.input.observed_properties.dependent_getters(&observed, reference.declaration);
        let direct = observed.contains(&reference.declaration);
        if !direct && dependent.is_empty() {
            return Ok(None);
        }
        let layout = self.input.aggregate_object_layout_from_fact(owner, &receiver_layout).ok_or(())?;
        let property = if direct {
            let field = layout.fields.get(access.index as usize).ok_or(())?;
            let always = self.query(beskid_queries::property_always(self.db, reference.declaration)).ok_or(())?;
            Some(ObservedProperty {
                declaration: reference.declaration,
                property_id: u64::from(reference.declaration.node.0),
                equality: if always { ObservationEquality::Always } else { self.equality_for_property(target, reference.declaration, reference.receiver, field.abi_type)? },
            })
        } else {
            None
        };
        let mut derived = Vec::with_capacity(dependent.len());
        for getter in dependent {
            let (callee, signature) = match beskid_queries::observation_getter_specialization(
                self.db,
                target,
                getter,
                reference.receiver,
                enclosing,
            )
            .map_err(|_| ())?
            {
                Some(instance) => {
                    (DirectCallee::specialized_item(getter, specialization_identity(&instance)), instance.signature)
                }
                None => (DirectCallee::item(getter), self.query(item_abi_signature(self.db, getter)).ok_or(())?),
            };
            derived.push(ObservedDerived {
                property: ObservedProperty {
                    declaration: getter,
                    property_id: u64::from(getter.node.0),
                    equality: self.equality_for_property(target, getter, reference.receiver, signature.result)?,
                },
                getter: callee,
                managed: matches!(signature.result, SemanticTypeId::STRING | SemanticTypeId::POINTER),
                signature: signature_for_item(self.isa.ok_or(())?, signature).ok_or(())?,
            });
        }
        Ok(Some(ObservedStore { receiver: reference.receiver, slot_offset: layout.observer_slot.ok_or(())?, property, derived }))
    }

    fn equality_for_property(
        &self,
        site: AstNodeKey,
        declaration: AstNodeKey,
        receiver: AstNodeKey,
        ty: SemanticTypeId,
    ) -> Result<ObservationEquality, ()> {
        Ok(match ty {
            SemanticTypeId::POINTER => {
                let instance = beskid_queries::observation_property_equality_instance(
                    self.db, site, declaration, receiver, self.current_item_specialization()
                ).map_err(|_| ())?.ok_or(())?;
                let callee = if instance.substitutions.is_empty() && instance.contract_witnesses.is_empty() {
                    DirectCallee::item(instance.declaration)
                } else {
                    DirectCallee::specialized_item(instance.declaration, specialization_identity(&instance))
                };
                ObservationEquality::Method(callee, signature_for_item(self.isa.ok_or(())?, instance.signature).ok_or(())?)
            }
        SemanticTypeId::STRING => ObservationEquality::String,
        SemanticTypeId::F32 | SemanticTypeId::F64 => ObservationEquality::Float,
        _ => ObservationEquality::Integer,
        })
    }
}
