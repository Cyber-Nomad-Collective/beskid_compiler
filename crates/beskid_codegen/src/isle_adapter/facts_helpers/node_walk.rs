//! Scalar types, literals, CLIF block bodies, and child traversal.

use super::super::*;

impl SyntaxNodeFacts<'_> {
    pub(in crate::isle_adapter) fn scalar_semantic_type(&self, key: AstNodeKey) -> Option<SemanticTypeId> {
        if self.query(implicit_method_receiver(self.db, key)).is_some() {
            return Some(SemanticTypeId::POINTER);
        }
        if self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::CallExpression)
            && let Some(instance) = self.generic_call_specialization_in_context(key)
        {
            return Some(instance.signature.result);
        }
        if self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::CallExpression)
            && let Some(signature) = self.query(call_abi_signature(self.db, key))
        {
            return Some(signature.result);
        }
        match self.query(node_kind(self.db, key)) {
            Some(beskid_queries::IndexedNodeKind::IndexExpression) => {
                return self.array_index_element_type_in_context(key);
            }
            Some(beskid_queries::IndexedNodeKind::AssignExpression) => {
                if let Some(element) = self.array_index_element_type_in_context(key) {
                    return Some(element);
                }
            }
            _ => {}
        }
        if self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::ForStatement) {
            return self.query(for_iterator_fact(self.db, key)).map(|fact| fact.element_type);
        }
        if let Some(binding) = self.specialized_pattern_binding(key) {
            return Some(match binding.payload {
                AggregateFieldShape::Scalar(semantic) => semantic,
                AggregateFieldShape::Nominal(_) => SemanticTypeId::POINTER,
            });
        }
        self.specialized_direct_parameter_type(key)
            .or_else(|| {
                self.query(beskid_queries::value_abi_type(self.db, key))
                    .or_else(|| {
                        self.query(cast_intents(self.db, key))
                            .and_then(|intents| intents.first().map(|intent| intent.to))
                    })
                    .or_else(|| {
                        (self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::LetStatement))
                            .then(|| {
                                self.raw_children(key)
                                    .into_iter()
                                    .find(|child| {
                                        self.query(node_kind(self.db, *child))
                                            == Some(beskid_queries::IndexedNodeKind::Identifier)
                                    })
                                    .and_then(|identifier| self.query(abi_type(self.db, identifier)))
                                    .or_else(|| {
                                        self.raw_children(key)
                                            .into_iter()
                                            .find(|child| {
                                                self.query(node_kind(self.db, *child))
                                                    == Some(beskid_queries::IndexedNodeKind::Identifier)
                                            })
                                            .and_then(|identifier| self.query(node_type(self.db, identifier)))
                                    })
                            })?
                    })
            })
            .or_else(|| {
                self.aggregate_field_access_in_context(key).and_then(|access| {
                    access.layout.fields.get(usize::try_from(access.index).ok()?).map(|(_, shape)| match shape {
                        AggregateFieldShape::Scalar(semantic) => *semantic,
                        AggregateFieldShape::Nominal(_) => SemanticTypeId::POINTER,
                    })
                })
            })
    }

    /// Project one pattern binding from the same immutable item specialization used for the
    /// enclosing match. This retains source ownership after pointer ABI erasure without HIR.
    pub(super) fn specialized_pattern_binding(&self, key: AstNodeKey) -> Option<beskid_queries::EnumMatchBindingFact> {
        (self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::PathExpression)).then_some(())?;
        let enclosing = self.current_item_specialization()?;
        self.query(beskid_queries::pattern_binding_specialization(self.db, key, enclosing.substitutions.clone()))
    }

    pub(in crate::isle_adapter) fn literal(&self, key: AstNodeKey) -> Option<LiteralFact> {
        self.query(literal_fact(self.db, key)).or_else(|| {
            self.query(child_nodes(self.db, key))?.iter().find_map(|child| self.query(literal_fact(self.db, *child)))
        })
    }

    pub(in crate::isle_adapter) fn clif_block_body_for(&self, key: AstNodeKey) -> Option<String> {
        self.query(clif_block_body(self.db, key)).map(|body| body.as_ref().to_string())
    }

    pub(in crate::isle_adapter) fn children(&self, key: AstNodeKey) -> Vec<AstNodeKey> {
        let children = self.raw_children(key);
        let children = if self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::TestDefinition) {
            children
                .into_iter()
                .filter(|child| {
                    self.query(node_kind(self.db, *child)) == Some(beskid_queries::IndexedNodeKind::Statement)
                })
                .collect()
        } else {
            children
        };
        children.into_iter().filter_map(|child| self.unwrap_transparent(child)).collect()
    }

    pub(in crate::isle_adapter) fn raw_children(&self, key: AstNodeKey) -> Vec<AstNodeKey> {
        self.query(child_nodes(self.db, key)).as_deref().into_iter().flatten().copied().collect()
    }

    pub(in crate::isle_adapter) fn unwrap_transparent(&self, mut key: AstNodeKey) -> Option<AstNodeKey> {
        loop {
            let kind = self.query(node_kind(self.db, key))?;
            // ElseBranch is a structural wrapper around Block or nested If; peel it so
            // emit_if_else can lower the concrete else arm directly.
            if !matches!(
                kind,
                beskid_queries::IndexedNodeKind::Statement
                    | beskid_queries::IndexedNodeKind::Expression
                    | beskid_queries::IndexedNodeKind::ElseBranch
            ) {
                return Some(key);
            }
            let children = self.query(child_nodes(self.db, key))?;
            key = *children.first()?;
        }
    }
}
