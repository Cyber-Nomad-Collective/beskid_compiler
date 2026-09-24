//! Collection operation and element-type facts.

use super::super::*;

impl SyntaxNodeFacts<'_> {
    pub(super) fn collection_operation_impl(&self, key: AstNodeKey) -> Option<CollectionOperation> {
        let operation = match beskid_queries::collection_operation(self.db, key) {
            Ok(Some(operation)) => operation,
            Err(_) => return Some(CollectionOperation::UnprovenMutationOwner),
            Ok(None) => return None,
        };
        Some(match operation {
            beskid_queries::CollectionOperation::Append { owner } => {
                let owner = match owner {
                    beskid_queries::CollectionMutationOwner::Local(slot) => {
                        CollectionMutationOwner::Local(LocalSlotId { owner_node: slot.owner.node.0, index: slot.index })
                    }
                    beskid_queries::CollectionMutationOwner::AggregateField { receiver, index, .. } => {
                        CollectionMutationOwner::AggregateField {
                            receiver: LocalSlotId { owner_node: receiver.owner.node.0, index: receiver.index },
                            field_index: index,
                        }
                    }
                };
                CollectionOperation::Append { owner }
            }
            beskid_queries::CollectionOperation::Capacity => CollectionOperation::Capacity,
            beskid_queries::CollectionOperation::Clear => CollectionOperation::Clear,
            beskid_queries::CollectionOperation::RemoveLast => CollectionOperation::RemoveLast,
        })
    }

    pub(super) fn collection_element_type_impl(&self, key: AstNodeKey) -> Option<Type> {
        let element = self.generic_call_specialization_in_context(key)?.substitutions.first()?.argument;
        map_signature_type(self.isa?, element)
    }
}
