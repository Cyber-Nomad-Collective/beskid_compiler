//! Exact composition facts: syntax query and frozen frontend authority must agree.

use super::super::*;

impl SyntaxNodeFacts<'_> {
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
        Some(CompositionLaunchPlan { site: key, slot_count: u32::try_from(plan.activation.len()).ok()? })
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
