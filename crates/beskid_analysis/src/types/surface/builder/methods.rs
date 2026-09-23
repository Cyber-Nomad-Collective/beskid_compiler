use crate::resolve::{ItemId, ResolvedType};
use crate::syntax::{MethodDefinition, SpanInfo, Spanned};

use super::state::TypeSurfaceBuilder;

impl<'a> TypeSurfaceBuilder<'a> {
    pub(super) fn seed_method_receiver(&mut self, method_span: SpanInfo, def: &Spanned<MethodDefinition>) {
        let Some(method_item_id) = self.canonical_item_id_for_span(method_span) else {
            return;
        };
        let Some(ResolvedType::Item(receiver_item_id)) = self.resolved_type_at(def.node.receiver_type.span) else {
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
