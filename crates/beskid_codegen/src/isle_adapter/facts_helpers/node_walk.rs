//! Scalar types, literals, CLIF block bodies, and child traversal.

use super::super::*;

impl SyntaxNodeFacts<'_> {
    pub(in crate::isle_adapter) fn scalar_semantic_type(&self, key: AstNodeKey) -> Option<SemanticTypeId> {
        if self.composition_injected_access(key).is_some() {
            return Some(SemanticTypeId::POINTER);
        }
        if self.query(implicit_method_receiver(self.db, key)).is_some() {
            return Some(SemanticTypeId::POINTER);
        }
        if self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::CallExpression)
            && let Some(instance) = self.generic_call_specialization_in_context(key)
        {
            return Some(instance.signature.result);
        }
        // `operand?` in a specialized body has the payload its operand has in that environment.
        if self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::TryExpression)
            && let Some(enclosing) = self.current_item_specialization()
            && let Some(fact) = self.query(beskid_queries::specialized_try_expression_fact(self.db, key, enclosing))
        {
            return Some(fact.payload_type);
        }
        if self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::CallExpression)
            && self.current_item_specialization().is_some()
            && let Some(call) = self.function_value_call_in_context(key)
        {
            return Some(call.signature.result);
        }
        if self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::CallExpression)
            && let Some(signature) = self.query(call_abi_signature(self.db, key))
        {
            return Some(signature.result);
        }
        match self.query(node_kind(self.db, key)) {
            Some(beskid_queries::IndexedNodeKind::IndexExpression) => {
                // An array element, else the `u8` byte of a string index.
                return self.array_index_element_type_in_context(key).or_else(|| {
                    self.query(beskid_queries::value_abi_type(self.db, key)).filter(|ty| *ty == SemanticTypeId::U8)
                });
            }
            Some(beskid_queries::IndexedNodeKind::AssignExpression) => {
                if let Some(element) = self.array_index_element_type_in_context(key) {
                    return Some(element);
                }
                // A field write inside a specialized body (`this.left = ...` in a method of a
                // generic type) has no target-neutral value ABI; the assigned field's shape in
                // the specialization types the assignment.
                if self.current_item_specialization().is_some()
                    && let Some(semantic) = self
                        .raw_children(key)
                        .first()
                        .copied()
                        .and_then(|target| self.unwrap_transparent(target))
                        .and_then(|target| self.field_semantic_type_in_context(target))
                {
                    return Some(semantic);
                }
            }
            _ => {}
        }
        if self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::ForStatement) {
            return self.for_iterator_in_context(key).map(|fact| fact.element_type);
        }
        if let Some(binding) = self.specialized_pattern_binding(key) {
            return Some(match binding.payload {
                AggregateFieldShape::Scalar(semantic) | AggregateFieldShape::ManagedReference(semantic) => semantic,
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
            .or_else(|| self.specialized_declared_local_type(key))
            .or_else(|| self.field_semantic_type_in_context(key))
            .or_else(|| self.specialized_match_value_type(key))
            .or_else(|| self.specialized_inferred_local_type(key))
    }

    /// The bodies of a match's arms in the specialization being lowered: its enum match fact in
    /// that environment, else its by-value scalar match fact.
    pub(in crate::isle_adapter) fn specialized_match_arm_bodies(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        (self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::MatchExpression)).then_some(())?;
        self.current_item_specialization()?;
        match self.enum_match_in_context(key) {
            Some(fact) => Some(fact.arms.iter().map(|arm| arm.body).collect()),
            None => Some(self.query(scalar_match(self.db, key))?.arms.iter().map(|arm| arm.body).collect()),
        }
    }

    /// The value type of a match in a specialized body that has no target-neutral type
    /// (`match this.slot { Option::Some(values) => values[0], Option::None => fallback }` in
    /// `Holder<T>`): the one type every arm that produces a value has in the specialization.
    /// Diverging arms do not constrain it; an arm of unknown or differing type proves nothing.
    fn specialized_match_value_type(&self, key: AstNodeKey) -> Option<SemanticTypeId> {
        let mut result = None;
        for body in self.specialized_match_arm_bodies(key)? {
            match self.scalar_semantic_type(body)? {
                SemanticTypeId::NEVER => {}
                semantic if result.is_none_or(|previous| previous == semantic) => result = Some(semantic),
                _ => return None,
            }
        }
        result
    }

    /// The type of an unannotated `let` inside a specialized body (`let x = match h.slot { ... }`):
    /// its initializer's type in the specialization. An annotated `let` is typed by its declared
    /// expectation alone.
    fn specialized_inferred_local_type(&self, key: AstNodeKey) -> Option<SemanticTypeId> {
        (self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::LetStatement)).then_some(())?;
        let enclosing = self.current_item_specialization()?;
        let initializer = self.let_initializer(key)?;
        if self.query(beskid_queries::expected_type_in_environment(self.db, initializer, enclosing)).is_some() {
            return None;
        }
        self.scalar_semantic_type(initializer).filter(|semantic| *semantic != SemanticTypeId::NEVER)
    }

    /// The declared type of an annotated `let` inside a specialized body (`T once = f(x)`), read
    /// through the one checking-direction query in the instance's environment: the initializer's
    /// `DeclaredLocal` expectation. A target-neutral declaration naming `T` has no ABI.
    fn specialized_declared_local_type(&self, key: AstNodeKey) -> Option<SemanticTypeId> {
        (self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::LetStatement)).then_some(())?;
        let enclosing = self.current_item_specialization()?;
        let initializer = self.let_initializer(key)?;
        let expected = self.query(beskid_queries::expected_type_in_environment(self.db, initializer, enclosing))?;
        matches!(expected.context, beskid_queries::ExpectedTypeContext::DeclaredLocal { .. })
            .then(|| expected.abi_type())
    }

    /// The semantic ABI of the field a field access (`this.left`, `value.field`) reads or writes,
    /// through the enclosing specialization when the receiver's layout needs one.
    fn field_semantic_type_in_context(&self, key: AstNodeKey) -> Option<SemanticTypeId> {
        let access = self.aggregate_field_access_in_context(key)?;
        access.layout.fields.get(usize::try_from(access.index).ok()?).map(|(_, shape)| match shape {
            AggregateFieldShape::Scalar(semantic) | AggregateFieldShape::ManagedReference(semantic) => *semantic,
            AggregateFieldShape::Nominal(_) => SemanticTypeId::POINTER,
        })
    }

    /// Project one pattern binding from the same immutable item specialization used for the
    /// enclosing match. This retains source ownership after pointer ABI erasure without HIR.
    pub(super) fn specialized_pattern_binding(&self, key: AstNodeKey) -> Option<beskid_queries::EnumMatchBindingFact> {
        (self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::PathExpression)).then_some(())?;
        let enclosing = self.current_item_specialization()?;
        self.query(beskid_queries::pattern_binding_specialization(self.db, key, enclosing))
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
