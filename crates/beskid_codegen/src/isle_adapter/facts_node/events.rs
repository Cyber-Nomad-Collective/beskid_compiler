use super::*;

impl SyntaxNodeFacts<'_> {
    pub(super) fn event_handler_local_impl(&self, key: AstNodeKey) -> Option<beskid_isle::EventHandlerLocalPlan> {
        let handler = self.query(beskid_queries::event_handler_lambda_for_local(self.db, key))?;
        let lambda = handler.lambda;
        let signature = handler.signature;
        let entry = self.event_lambda_entry_impl(lambda)?;
        let parameters = signature
            .parameters
            .iter()
            .copied()
            .map(|ty| map_signature_type(self.isa?, ty))
            .collect::<Option<Vec<_>>>()?;
        let result =
            if matches!(signature.result, beskid_queries::SemanticTypeId::UNIT | beskid_queries::SemanticTypeId::NEVER)
            {
                None
            } else {
                Some(map_signature_type(self.isa?, signature.result)?)
            };
        Some(beskid_isle::EventHandlerLocalPlan {
            lambda,
            trampoline: entry.trampoline,
            closure_environment: entry.closure_environment,
            parameters: parameters.into(),
            result,
        })
    }

    pub(super) fn assignment_kind_impl(&self, key: AstNodeKey) -> Option<beskid_isle::AssignmentKind> {
        if let Some(plan) = self.event_operation_impl(key) {
            return match plan.operation {
                beskid_isle::EventOperation::Subscribe => Some(beskid_isle::AssignmentKind::EventSubscribe),
                beskid_isle::EventOperation::UnsubscribeFirst => {
                    Some(beskid_isle::AssignmentKind::EventUnsubscribeFirst)
                }
                beskid_isle::EventOperation::Raise => None,
            };
        }
        let target = self.query(beskid_queries::child_nodes(self.db, key))?.first().copied()?;
        let target = self.unwrap_transparent(target)?;
        if self.aggregate_field_access_in_context(target).is_some() {
            return Some(beskid_isle::AssignmentKind::Field);
        }
        match self.query(beskid_queries::node_kind(self.db, target))? {
            beskid_queries::IndexedNodeKind::PathExpression => Some(beskid_isle::AssignmentKind::Local),
            beskid_queries::IndexedNodeKind::MemberExpression => Some(beskid_isle::AssignmentKind::Field),
            beskid_queries::IndexedNodeKind::IndexExpression => Some(beskid_isle::AssignmentKind::Index),
            _ => None,
        }
    }

    pub(super) fn event_operation_impl(&self, key: AstNodeKey) -> Option<beskid_isle::EventOperationPlan> {
        let fact = self.query(beskid_queries::event_operation(self.db, key))?;
        // The v0.5 policy for declarations without an explicit bound is still under release
        // review. Do not smuggle the query's zero sentinel into the canonical positive-capacity ABI.
        if fact.capacity == 0 {
            return None;
        }
        let signature = fact.delegate_signature?;
        let operation = match fact.operation {
            beskid_queries::EventOperationKind::Subscribe => beskid_isle::EventOperation::Subscribe,
            beskid_queries::EventOperationKind::UnsubscribeFirst => beskid_isle::EventOperation::UnsubscribeFirst,
            beskid_queries::EventOperationKind::Raise => beskid_isle::EventOperation::Raise,
        };
        Some(beskid_isle::EventOperationPlan {
            operation,
            operation_node: fact.operation_node,
            receiver: fact.receiver,
            receiver_slot: (self.query(node_kind(self.db, fact.receiver))
                == Some(beskid_queries::IndexedNodeKind::MethodDefinition))
            .then_some(super::super::context::IMPLICIT_METHOD_RECEIVER_SLOT),
            declaration: fact.declaration,
            field: fact.field,
            slot_offset: fact.slot_offset,
            capacity: fact.capacity,
            handler: fact.handler,
            handler_lambda: fact.handler_lambda,
            arguments: fact.arguments,
            delegate_parameters: signature.parameters,
            delegate_result: signature.result,
        })
    }
}
