//! Exact composition facts: syntax query and frozen frontend authority must agree.

use beskid_isle::{CompositionInjectionPlan, CompositionRegistrationPlan};

use super::super::*;

impl SyntaxNodeFacts<'_> {
    pub(in crate::isle_adapter) fn composition_injected_access(
        &self,
        key: AstNodeKey,
    ) -> Option<(beskid_queries::CompositionInjectedAccessFact, u32, u32, u8)> {
        matches!(
            self.query(beskid_queries::node_kind(self.db, key)),
            Some(beskid_queries::IndexedNodeKind::PathExpression | beskid_queries::IndexedNodeKind::MemberExpression)
        )
        .then_some(())?;
        let (plan, snapshot) = self.input.composition_authority()?;
        let access = self.query(beskid_queries::composition_injected_field_access(self.db, key))?;
        let field = self.query(beskid_queries::composition_injection_field(self.db, access.field))?;
        (access.site == key && field.field == access.field && field.owner_type == access.owner_type).then_some(())?;
        (access.field.unit == self.input.typed_program().entry
            && access.field.generation == self.input.typed_program().generation)
            .then_some(())?;
        let owner_id = plan
            .singulars
            .iter()
            .find(|entry| entry.field_node_id == access.field.node)
            .map(|entry| entry.owner_registration_id)
            .or_else(|| {
                plan.plurals
                    .iter()
                    .find(|entry| entry.field_node_id == access.field.node)
                    .map(|entry| entry.owner_registration_id)
            })?;
        let owner = snapshot.registrations.iter().find(|entry| entry.id == owner_id)?;
        let registration = AstNodeKey { node: owner.source_node_id, ..key };
        let declared = self.query(beskid_queries::composition_registration(self.db, registration))?;
        (declared.declaration == access.owner_type && declared.implementation.as_ref() == owner.implementation)
            .then_some(())?;
        let layout = self.input.aggregate_object_layout(access.owner_type)?;
        let (_, offset) = layout.injected_fields.iter().find(|(source, _)| *source == access.field)?;
        Some((
            access,
            u32::try_from(*offset).ok()?,
            u32::try_from(layout.object_size).ok()?,
            layout.object_alignment.ilog2() as u8,
        ))
    }

    pub(super) fn composition_launch_impl(&self, key: AstNodeKey) -> Option<CompositionLaunchPlan> {
        let (plan, snapshot) = self.input.composition_authority()?;
        let fact = self.query(beskid_queries::composition_launch(self.db, key))?;
        if key.unit != self.input.typed_program().entry
            || key.generation != self.input.typed_program().generation
            || fact.site != key
            || fact.host.as_ref() != snapshot.launched_host
            || plan.launched_host != snapshot.launched_host
            || self.query(beskid_queries::node_span(self.db, key)) != snapshot.launch_span
        {
            return None;
        }
        let mut registrations = Vec::with_capacity(plan.activation.len());
        for activation in &plan.activation {
            let registration = snapshot.registrations.iter().find(|entry| entry.id == activation.registration_id)?;
            let source = AstNodeKey { node: registration.source_node_id, ..key };
            let fact = self.query(beskid_queries::composition_registration(self.db, source))?;
            (fact.site == source && fact.implementation.as_ref() == registration.implementation).then_some(())?;
            let allocation = self.input.composition_registration_static_plan(registration.id)?;
            let layout = self.input.aggregate_object_layout(fact.declaration)?;
            let mut injections = Vec::new();
            for singular in plan.singulars.iter().filter(|entry| entry.owner_registration_id == registration.id) {
                let field = AstNodeKey { node: singular.field_node_id, ..key };
                let field_fact = self.query(beskid_queries::composition_injection_field(self.db, field))?;
                (field_fact.owner_type == fact.declaration && !field_fact.is_plural).then_some(())?;
                let (_, offset) = layout.injected_fields.iter().find(|(source, _)| *source == field)?;
                (singular.target_slot.0 < activation.slot.0).then_some(())?;
                injections.push(CompositionInjectionPlan {
                    field,
                    field_offset: u32::try_from(*offset).ok()?,
                    target_slots: vec![singular.target_slot.0],
                    plural_allocation_request_symbol: None,
                });
            }
            for plural in plan.plurals.iter().filter(|entry| entry.owner_registration_id == registration.id) {
                let field = AstNodeKey { node: plural.field_node_id, ..key };
                let field_fact = self.query(beskid_queries::composition_injection_field(self.db, field))?;
                (field_fact.owner_type == fact.declaration && field_fact.is_plural).then_some(())?;
                let (_, offset) = layout.injected_fields.iter().find(|(source, _)| *source == field)?;
                plural.target_slots.iter().all(|slot| slot.0 < activation.slot.0).then_some(())?;
                let array = self.input.composition_plural_static_plan(field, plural.target_slots.len())?;
                injections.push(CompositionInjectionPlan {
                    field,
                    field_offset: u32::try_from(*offset).ok()?,
                    target_slots: plural.target_slots.iter().map(|slot| slot.0).collect(),
                    plural_allocation_request_symbol: Some(array.allocation_request_symbol.into()),
                });
            }
            (injections.len() == layout.injected_fields.len()).then_some(())?;
            let unique_fields =
                injections.iter().map(|injection| injection.field).collect::<std::collections::HashSet<_>>();
            (unique_fields.len() == injections.len()).then_some(())?;
            registrations.push(CompositionRegistrationPlan {
                slot: activation.slot.0,
                allocation_request_symbol: allocation.allocation_request_symbol.into(),
                injections,
            });
        }
        Some(CompositionLaunchPlan { site: key, slot_count: u32::try_from(plan.activation.len()).ok()?, registrations })
    }

    pub(super) fn composition_scope_impl(&self, key: AstNodeKey) -> Option<CompositionScopePlan> {
        let (plan, snapshot) = self.input.composition_authority()?;
        let fact = self.query(beskid_queries::composition_scope(self.db, key))?;
        let (scope_id, _) = snapshot.scope_names.iter().find(|(_, name)| name.as_str() == fact.scope_name.as_ref())?;
        if key.unit != self.input.typed_program().entry
            || key.generation != self.input.typed_program().generation
            || fact.site != key
            || !plan.scope_parents.contains_key(scope_id)
        {
            return None;
        }
        Some(CompositionScopePlan { site: key, scope_id: scope_id.0, body: fact.body })
    }
}
