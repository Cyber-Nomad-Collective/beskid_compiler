//! Operator, conversion, semantic, scalar, managed-reference, try, and range facts.

use super::super::*;

impl SyntaxNodeFacts<'_> {
    pub(super) fn operator_fact_impl(&self, key: AstNodeKey) -> Option<OperatorFact> {
        let operator = self.query(operator_fact(self.db, key))?;
        let specialized_string_operands =
            matches!(operator, beskid_queries::OperatorFact::Eq | beskid_queries::OperatorFact::NotEq)
                && self.child(key, 0).and_then(|operand| self.scalar_semantic_type(operand))
                    == Some(SemanticTypeId::STRING)
                && self.child(key, 1).and_then(|operand| self.scalar_semantic_type(operand))
                    == Some(SemanticTypeId::STRING);
        let specialized_enum_operands =
            matches!(operator, beskid_queries::OperatorFact::Eq | beskid_queries::OperatorFact::NotEq)
                && !specialized_string_operands
                && self.binary_enum_layout(key).is_some();
        Some(match (operator, specialized_string_operands, specialized_enum_operands) {
            (beskid_queries::OperatorFact::Eq, _, true) => OperatorFact::EnumEq,
            (beskid_queries::OperatorFact::NotEq, _, true) => OperatorFact::EnumNotEq,
            (beskid_queries::OperatorFact::Eq, true, _) => OperatorFact::StringEq,
            (beskid_queries::OperatorFact::NotEq, true, _) => OperatorFact::StringNotEq,
            (operator, _, _) => map_operator_fact(operator),
        })
    }

    pub(super) fn primitive_numeric_conversion_impl(
        &self,
        key: AstNodeKey,
    ) -> Option<(SemanticTypeId, SemanticTypeId)> {
        self.query(beskid_queries::primitive_numeric_conversion(self.db, key)).map(|fact| (fact.from, fact.to))
    }

    pub(super) fn semantic_type_impl(&self, key: AstNodeKey) -> Option<SemanticTypeId> {
        if let Some(CallLowering::CorelibService(service)) = self.query(call_lowering(self.db, key))
            && beskid_abi::runtime_source::canonical_corelib_service_value_dispatch(service).is_some()
        {
            return self.typed_corelib_value_service(key, service).map(|(_, result)| result);
        }
        if let Some(specialization) = self.generic_call_specialization_in_context(key) {
            return Some(specialization.signature.result);
        }
        self.specialized_direct_parameter_type(key).or_else(|| self.scalar_semantic_type(key))
    }

    pub(super) fn managed_reference_impl(&self, key: AstNodeKey) -> Option<ManagedReferenceFact> {
        self.managed_reference_in_context(key)
    }

    pub(super) fn try_expression_fact_impl(&self, key: AstNodeKey) -> Option<beskid_queries::TryExpressionFact> {
        self.query(try_expression_fact(self.db, key))
    }

    pub(super) fn try_return_layout_impl(&self, key: AstNodeKey) -> Option<EnumLayout> {
        self.enum_layout_from_fact(&self.query(try_expression_fact(self.db, key))?.return_layout)
    }

    pub(super) fn scalar_type_impl(&self, key: AstNodeKey) -> Option<Type> {
        if self.node_kind(key) == Some(NodeKind::StructLiteralExpression)
            && self.query(aggregate_literal_declaration(self.db, key)).is_some()
        {
            return self.isa.map(|isa| isa.pointer_type());
        }
        if self.node_kind(key) == Some(NodeKind::ArrayLiteralExpression) {
            return map_signature_type(self.isa?, self.query(abi_type(self.db, key))?);
        }
        if self.node_kind(key) == Some(NodeKind::EnumLiteralExpression)
            && (self.query(enum_constructor(self.db, key)).is_some()
                || self.specialized_enum_constructor(key).is_some())
        {
            return self.isa.map(|isa| isa.pointer_type());
        }
        if let Some((_, intrinsic)) = self.runtime_intrinsic(key) {
            let signature = signature_for_runtime_intrinsic(self.isa?, intrinsic)?;
            return signature.returns.first().map(|param| param.value_type);
        }
        if self.scheduler_compiler_operation(key).is_some() {
            return self
                .call_signature(key)
                .and_then(|signature| signature.returns.first().map(|param| param.value_type));
        }
        if self.node_kind(key) == Some(NodeKind::CallExpression)
            && let Some(signature) = self.call_signature(key)
        {
            return signature.returns.first().map(|parameter| parameter.value_type);
        }
        let contextual = self.query(contextual_integer_literal_abi_type(self.db, key));
        let semantic = contextual
            .or_else(|| {
                (self.node_kind(key) == Some(NodeKind::CallExpression))
                    .then(|| self.scalar_semantic_type(key))
                    .flatten()
            })
            .or_else(|| self.query(call_argument_abi_type(self.db, key)))
            .or_else(|| self.scalar_semantic_type(key))
            .or_else(|| Some(self.query(call_abi_signature(self.db, key))?.result))?;
        if matches!(semantic, SemanticTypeId::WORD | SemanticTypeId::POINTER | SemanticTypeId::STRING) {
            return self.isa.map(|isa| isa.pointer_type());
        }
        map_scalar_type(semantic)
    }

    pub(super) fn range_fact_impl(&self, key: AstNodeKey) -> Option<beskid_isle::RangeFact> {
        let range = self.query(range_for_fact(self.db, key))?;
        Some(beskid_isle::RangeFact::new(range.start, range.end, 1, false))
    }
}
