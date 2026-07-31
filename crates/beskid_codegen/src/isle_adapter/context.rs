use super::*;

/// Query-backed facts for generated ISLE selection.
///
/// Every answer is read from the generation-safe syntax authority registered by the typed
/// program. Missing or not-yet-ported facts remain unavailable to ISLE instead of falling back
/// to HIR or hand-built test facts.
pub struct SyntaxNodeFacts<'db> {
    pub(super) db: &'db dyn Db,
    pub(super) input: &'db CodegenInput<'db>,
    pub(super) isa: Option<&'db dyn TargetIsa>,
    pub(super) item_specializations: HashMap<AstNodeKey, GenericCallSpecialization>,
}

impl<'db> SyntaxNodeFacts<'db> {
    pub fn new(input: &'db CodegenInput<'db>) -> Self {
        Self { db: input.database(), input, isa: None, item_specializations: HashMap::new() }
    }

    pub(super) fn new_with_isa(input: &'db CodegenInput<'db>, isa: &'db dyn TargetIsa) -> Self {
        Self { db: input.database(), input, isa: Some(isa), item_specializations: HashMap::new() }
    }

    pub(super) fn new_with_item_specialization(
        input: &'db CodegenInput<'db>,
        isa: &'db dyn TargetIsa,
        item: AstNodeKey,
        specialization: GenericCallSpecialization,
    ) -> Self {
        Self {
            db: input.database(),
            input,
            isa: Some(isa),
            item_specializations: HashMap::from([(item, specialization)]),
        }
    }

    pub(super) fn query<T>(&self, result: beskid_queries::SemanticQueryResult<T>) -> Option<T> {
        result.ok().flatten()
    }

    pub(super) fn inline_closure_environment(
        &self,
        site: AstNodeKey,
        lambda: AstNodeKey,
    ) -> Option<InlineClosureEnvironment> {
        let isa = self.isa?;
        let authority = self.input.closure_lowering_authority(site, lambda)?;
        let captures = authority
            .plan
            .captures
            .iter()
            .map(|field| {
                Some(InlineCaptureField {
                    local_slot: LocalSlotId {
                        owner_node: field.capture.slot.owner.node.0,
                        index: field.capture.slot.index,
                    },
                    field_offset: u32::try_from(field.field_offset).ok()?,
                    pointer_map_index: field.pointer_map_index,
                    value_type: map_signature_type(isa, field.abi_type)?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(InlineClosureEnvironment {
            allocation_request_symbol: authority.plan.allocation_request_symbol.into(),
            descriptor_symbol: authority.plan.descriptor_symbol.into(),
            root_slot_index: authority.root.slot_index,
            captures,
        })
    }

    pub(super) fn specialized_direct_parameter_type(&self, key: AstNodeKey) -> Option<SemanticTypeId> {
        self.item_specializations
            .iter()
            .find_map(|(item, specialization)| specialized_local_abi_type(self.db, key, *item, specialization))
    }

    pub(super) fn contextual_expression_is_string(&self, key: AstNodeKey) -> bool {
        if self.query(abi_type(self.db, key)) == Some(SemanticTypeId::STRING)
            || self.query(node_type(self.db, key)) == Some(SemanticTypeId::STRING)
            || self.specialized_direct_parameter_type(key) == Some(SemanticTypeId::STRING)
            || matches!(self.query(literal_fact(self.db, key)), Some(LiteralFact::String(_)))
        {
            return true;
        }
        if self.query(node_kind(self.db, key)) != Some(beskid_queries::IndexedNodeKind::BinaryExpression) {
            return false;
        }
        matches!(self.query(operator_fact(self.db, key)), Some(beskid_queries::OperatorFact::StringAdd))
            || (self.query(operator_fact(self.db, key)) == Some(beskid_queries::OperatorFact::Add)
                && self.children(key).into_iter().any(|child| self.contextual_expression_is_string(child)))
    }
}
