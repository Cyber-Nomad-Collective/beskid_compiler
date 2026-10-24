//! Exact composition facts: syntax query and frozen frontend authority must agree.

use beskid_analysis::composition::{BindingPlan, CompositionHookPlan, CompositionSnapshot, ScopeId};
use beskid_isle::{CompositionHookCall, CompositionInjectionPlan, CompositionRegistrationPlan};

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
        (access.field.generation == self.input.typed_program().generation).then_some(())?;
        let owner_id = plan
            .singulars
            .iter()
            .find(|entry| self.input.composition_unit_key(&entry.field_unit, entry.field_node_id) == access.field)
            .map(|entry| entry.owner_registration_id)
            .or_else(|| {
                plan.plurals
                    .iter()
                    .find(|entry| self.input.composition_unit_key(&entry.field_unit, entry.field_node_id) == access.field)
                    .map(|entry| entry.owner_registration_id)
            })?;
        let owner = snapshot.registrations.iter().find(|entry| entry.id == owner_id)?;
        let registration = self.input.composition_unit_key(&owner.unit, owner.source_node_id);
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
            // `launch Module.AppHost()` names the host by a path; the snapshot keys it by name.
            || fact.host.rsplit('.').next() != Some(snapshot.launched_host.as_str())
            || plan.launched_host != snapshot.launched_host
            || self.query(beskid_queries::node_span(self.db, key)) != snapshot.launch_span
        {
            return None;
        }
        let mut registrations = Vec::with_capacity(plan.activation.len());
        for activation in &plan.activation {
            let registration = snapshot.registrations.iter().find(|entry| entry.id == activation.registration_id)?;
            let source = self.input.composition_unit_key(&registration.unit, registration.source_node_id);
            let fact = self.query(beskid_queries::composition_registration(self.db, source))?;
            (fact.site == source && fact.implementation.as_ref() == registration.implementation).then_some(())?;
            let allocation = self.input.composition_registration_static_plan(registration.id)?;
            let layout = self.input.aggregate_object_layout(fact.declaration)?;
            let mut injections = Vec::new();
            for singular in plan.singulars.iter().filter(|entry| entry.owner_registration_id == registration.id) {
                let field = self.input.composition_unit_key(&singular.field_unit, singular.field_node_id);
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
                let field = self.input.composition_unit_key(&plural.field_unit, plural.field_node_id);
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
        let global = |hook: &&CompositionHookPlan| hook.scope_id == ScopeId::GLOBAL;
        let startup = plan
            .init_hooks
            .iter()
            .filter(global)
            .chain(plan.startup_hooks.iter().filter(global))
            .map(|hook| self.composition_hook_call(key, hook, plan, snapshot))
            .collect::<Option<Vec<_>>>()?;
        // A `startup` outside global scope is rejected by composition (E1717); never drop one.
        (plan.startup_hooks.iter().all(|hook| hook.scope_id == ScopeId::GLOBAL)).then_some(())?;
        let shutdown = plan
            .disposal_hooks
            .iter()
            .filter(global)
            .map(|hook| self.composition_hook_call(key, hook, plan, snapshot))
            .collect::<Option<Vec<_>>>()?;
        Some(CompositionLaunchPlan {
            site: key,
            slot_count: u32::try_from(plan.activation.len()).ok()?,
            registrations,
            startup,
            shutdown,
        })
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
        let parent_scope_id = plan.scope_parents.get(scope_id)?.map_or(0, |parent| parent.0);
        let calls = |hooks: &[CompositionHookPlan]| {
            hooks
                .iter()
                .filter(|hook| hook.scope_id == *scope_id)
                .map(|hook| self.composition_hook_call(key, hook, plan, snapshot))
                .collect::<Option<Vec<_>>>()
        };
        let init = calls(&plan.init_hooks)?;
        let dispose = calls(&plan.disposal_hooks)?;
        Some(CompositionScopePlan { site: key, scope_id: scope_id.0, parent_scope_id, body: fact.body, init, dispose })
    }

    /// One planned hook call: the generated hook function and, for a hook with parameters, the
    /// slot and injected-field offsets of its argument object in parameter order.
    fn composition_hook_call(
        &self,
        _site: AstNodeKey,
        hook: &CompositionHookPlan,
        plan: &BindingPlan,
        snapshot: &CompositionSnapshot,
    ) -> Option<CompositionHookCall> {
        let function = self.input.composition_unit_key(&hook.unit, hook.function_node_id);
        (self.query(node_kind(self.db, function)) == Some(beskid_queries::IndexedNodeKind::FunctionDefinition))
            .then_some(())?;
        let signature = signature_for_item(self.isa?, self.query(item_abi_signature(self.db, function))?)?;
        let (arguments_slot, argument_offsets) = match hook.arguments_registration_id {
            None => (None, Vec::new()),
            Some(id) => {
                let slot = plan.slot_for_registration(id)?;
                let registration = snapshot.registrations.iter().find(|entry| entry.id == id)?;
                let source = self.input.composition_unit_key(&registration.unit, registration.source_node_id);
                let fact = self.query(beskid_queries::composition_registration(self.db, source))?;
                (fact.site == source && fact.implementation.as_ref() == registration.implementation).then_some(())?;
                let layout = self.input.aggregate_object_layout(fact.declaration)?;
                let mut fields = layout
                    .injected_fields
                    .iter()
                    .map(|(field, offset)| {
                        let field = self.query(beskid_queries::composition_injection_field(self.db, *field))?;
                        (field.owner_type == fact.declaration).then_some(())?;
                        Some((field.ordinal, u32::try_from(*offset).ok()?))
                    })
                    .collect::<Option<Vec<_>>>()?;
                fields.sort_by_key(|(ordinal, _)| *ordinal);
                (Some(slot.0), fields.into_iter().map(|(_, offset)| offset).collect::<Vec<_>>())
            }
        };
        // Every argument is one service reference loaded from its injected field.
        let pointer = self.isa?.pointer_type();
        (signature.params.len() == argument_offsets.len()
            && signature.params.iter().all(|parameter| parameter.value_type == pointer))
        .then_some(())?;
        Some(CompositionHookCall { function: DirectCallee::item(function), signature, arguments_slot, argument_offsets })
    }
}
