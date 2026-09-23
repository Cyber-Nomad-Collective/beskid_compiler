use crate::resolve::{ItemId, ResolvedType};
use crate::syntax::{MethodDefinition, SpanInfo, Spanned};
use crate::types::TypeInfo;

use super::state::TypeSurfaceBuilder;

impl<'a> TypeSurfaceBuilder<'a> {
    pub(super) fn seed_method_receiver(&mut self, method_span: SpanInfo, def: &Spanned<MethodDefinition>) {
        let Some(method_item_id) = self.canonical_item_id_for_span(method_span) else {
            return;
        };
        // A dependency unit may carry no source-scoped fact for the receiver span (its bodies
        // were not resolved); the declared receiver type path is then the authority, exactly as
        // for every other declaration type this surface resolves.
        let receiver_item_id = match self.resolved_type_at(def.node.receiver_type.span) {
            Some(ResolvedType::Item(item_id)) => Some(item_id),
            _ => self.type_id_for_type(&def.node.receiver_type).and_then(|type_id| match self.types.get(type_id) {
                Some(TypeInfo::Named(item_id)) => Some(*item_id),
                Some(TypeInfo::Applied { base, .. }) => Some(*base),
                _ => None,
            }),
        };
        let Some(receiver_item_id) = receiver_item_id else {
            return;
        };
        self.surface.methods_by_receiver.insert((receiver_item_id, def.node.name.node.name.clone()), method_item_id);
    }

    pub(super) fn seed_owned_method_receiver(
        &mut self,
        receiver_item_id: ItemId,
        method_span: SpanInfo,
        def: &Spanned<MethodDefinition>,
    ) {
        let Some(method_item_id) = self.canonical_item_id_for_span(method_span) else {
            return;
        };
        self.surface.methods_by_receiver.insert((receiver_item_id, def.node.name.node.name.clone()), method_item_id);
    }
}
