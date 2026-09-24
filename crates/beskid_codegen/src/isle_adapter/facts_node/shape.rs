//! Node kinds, children, statements, blocks, locals, and cleanup facts.

use super::super::*;

impl SyntaxNodeFacts<'_> {
    pub(super) fn scoped_cleanup_impl(&self, key: AstNodeKey) -> Option<beskid_isle::ScopedCleanupPlan> {
        let fact = self.query(beskid_queries::scoped_cleanup(self.db, key))?;
        if fact.diagnostic.is_some() {
            return None;
        }
        let dispose = fact.dispose?;
        let conversion = match fact.conversion {
            Some(item) => Some((
                DirectCallee::item(item),
                signature_for_item(self.isa?, self.query(item_abi_signature(self.db, item))?)?,
            )),
            None => None,
        };
        Some(beskid_isle::ScopedCleanupPlan {
            binding: fact.binding,
            body: fact.body,
            dispose: DirectCallee::item(dispose),
            dispose_signature: signature_for_item(self.isa?, self.query(item_abi_signature(self.db, dispose))?)?,
            dispose_layout: self.enum_layout_from_fact(&fact.dispose_layout?)?,
            conversion,
            enclosing_layout: self.enum_layout_from_fact(&fact.enclosing_layout?)?,
            allocation: self.managed_struct_allocation(key)?,
            converted_error_managed: fact.converted_error_managed,
        })
    }

    pub(super) fn node_kind_impl(&self, key: AstNodeKey) -> Option<NodeKind> {
        if self.aggregate_field_access_in_context(key).is_some() {
            return Some(NodeKind::FieldExpression);
        }
        if self.query(range_for_fact(self.db, key)).is_some() {
            return Some(NodeKind::RangeExpression);
        }
        self.query(node_kind(self.db, key)).and_then(map_node_kind)
    }

    pub(super) fn literal_kind_impl(&self, key: AstNodeKey) -> Option<LiteralKind> {
        self.literal(key).map(|fact| match fact {
            LiteralFact::Integer(_) => LiteralKind::Integer,
            LiteralFact::Float(_) => LiteralKind::Float,
            LiteralFact::String(_) => LiteralKind::String,
            LiteralFact::Char(_) => LiteralKind::Char,
            LiteralFact::Bool(_) => LiteralKind::Boolean,
        })
    }

    pub(super) fn child_impl(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
        if index == 0
            && let Some(access) = self.aggregate_field_access_in_context(key)
        {
            return Some(access.receiver);
        }
        let children = if self.node_kind(key) == Some(NodeKind::TestDefinition) {
            self.query(test_statement_nodes(self.db, key))?
        } else if self.node_kind(key) == Some(NodeKind::BlockExpression) {
            self.query(block_statement_nodes(self.db, key))?
        } else if self.node_kind(key) == Some(NodeKind::BinaryExpression) {
            self.children(key)
                .iter()
                .copied()
                .filter(|child| {
                    !matches!(self.query(node_kind(self.db, *child)), Some(beskid_queries::IndexedNodeKind::BinaryOp))
                })
                .collect()
        } else if self.node_kind(key) == Some(NodeKind::UnaryExpression) {
            // Operand-only view: UnaryOp is selected via `operator_fact`, not as a child.
            self.children(key)
                .iter()
                .copied()
                .filter(|child| {
                    !matches!(self.query(node_kind(self.db, *child)), Some(beskid_queries::IndexedNodeKind::UnaryOp))
                })
                .collect()
        } else if self.node_kind(key) == Some(NodeKind::ForStatement) {
            self.children(key)
                .iter()
                .copied()
                .filter(|child| {
                    self.query(node_kind(self.db, *child)) != Some(beskid_queries::IndexedNodeKind::Identifier)
                })
                .collect()
        } else {
            self.children(key).into()
        };
        children.get(usize::from(index)).copied().and_then(|child| self.unwrap_transparent(child))
    }

    pub(super) fn statement_count_impl(&self, key: AstNodeKey) -> Option<u8> {
        let kind = self.node_kind(key)?;
        match kind {
            NodeKind::BlockExpression => {
                let nodes = self.query(block_statement_nodes(self.db, key))?;
                let len =
                    if self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::BlockExpression) {
                        nodes.last().map_or(nodes.len(), |result_statement| {
                            if self.node_kind(*result_statement) == Some(NodeKind::ExpressionStatement) {
                                nodes.len() - 1
                            } else {
                                nodes.len()
                            }
                        })
                    } else {
                        nodes.len()
                    };
                if len > u8::MAX as usize {
                    return None;
                }
                u8::try_from(len).ok()
            }
            NodeKind::TestDefinition => {
                let nodes = self.query(test_statement_nodes(self.db, key))?;
                let len = nodes.len();
                if len > u8::MAX as usize {
                    return None;
                }
                u8::try_from(len).ok()
            }
            _ => None,
        }
    }

    pub(super) fn block_result_impl(&self, key: AstNodeKey) -> Option<AstNodeKey> {
        (self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::BlockExpression)).then_some(())?;
        let result_statement = *self.query(block_statement_nodes(self.db, key))?.last()?;
        (self.node_kind(result_statement) == Some(NodeKind::ExpressionStatement)).then_some(())?;
        self.child(result_statement, 0)
    }

    pub(super) fn let_initializer_impl(&self, key: AstNodeKey) -> Option<AstNodeKey> {
        (self.node_kind(key) == Some(NodeKind::LetStatement))
            .then(|| self.children(key).last().copied())?
            .and_then(|initializer| self.unwrap_transparent(initializer))
    }

    pub(super) fn local_slot_impl(&self, key: AstNodeKey) -> Option<LocalSlotId> {
        match self.query(node_kind(self.db, key))? {
            beskid_queries::IndexedNodeKind::PathExpression => {
                if self.query(implicit_method_receiver(self.db, key)).is_some() {
                    return Some(super::super::context::IMPLICIT_METHOD_RECEIVER_SLOT);
                }
                let declaration = self
                    .query(resolved_local(self.db, key))
                    .map(|resolved| resolved.declaration)
                    .or_else(|| self.query(nominal_member_receiver(self.db, key)))?;
                self.query(local_slot(self.db, declaration))
                    .map(|slot| LocalSlotId { owner_node: slot.owner.node.0, index: slot.index })
            }
            beskid_queries::IndexedNodeKind::LetStatement => self
                .raw_children(key)
                .into_iter()
                .find(|child| {
                    self.query(node_kind(self.db, *child)) == Some(beskid_queries::IndexedNodeKind::Identifier)
                })
                .and_then(|identifier| self.query(local_slot(self.db, identifier)))
                .map(|slot| LocalSlotId { owner_node: slot.owner.node.0, index: slot.index }),
            beskid_queries::IndexedNodeKind::ForStatement => self
                .query(for_iterator_fact(self.db, key))
                .and_then(|fact| self.query(local_slot(self.db, fact.declaration)))
                .map(|slot| LocalSlotId { owner_node: slot.owner.node.0, index: slot.index }),
            _ => None,
        }
    }

    pub(super) fn mutable_local_assignment_slot_impl(&self, key: AstNodeKey) -> Option<LocalSlotId> {
        self.query(mutable_local_assignment(self.db, key))
            .map(|assignment| LocalSlotId { owner_node: assignment.slot.owner.node.0, index: assignment.slot.index })
    }

    pub(super) fn index_target_is_string_impl(&self, key: AstNodeKey) -> bool {
        self.child(key, 0).and_then(|target| self.scalar_semantic_type(target)) == Some(SemanticTypeId::STRING)
    }

    pub(super) fn clif_block_body_impl(&self, key: AstNodeKey) -> Option<String> {
        self.clif_block_body_for(key)
    }
}
