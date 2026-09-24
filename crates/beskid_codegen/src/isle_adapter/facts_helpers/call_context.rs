//! Generic call specialization, runtime intrinsic, scheduler, and parameter facts.

use super::super::*;

impl SyntaxNodeFacts<'_> {
    pub(in crate::isle_adapter) fn generic_call_specialization_in_context(
        &self,
        key: AstNodeKey,
    ) -> Option<beskid_queries::GenericSpecializationInstance> {
        if let Some(enclosing) = self.current_item_specialization()
            && let Some(specialization) =
                self.query(generic_call_specialization_in_environment(self.db, key, enclosing))
        {
            return Some(specialization);
        }
        self.query(generic_call_specialization(self.db, key))
            .and_then(|specialization| self.query(generic_call_specialization_instance(self.db, specialization)))
    }

    pub(in crate::isle_adapter) fn runtime_intrinsic(
        &self,
        key: AstNodeKey,
    ) -> Option<(u32, &beskid_abi::abi_v5::RuntimeIntrinsic)> {
        let name = self.query(runtime_intrinsic_name(self.db, key))?;
        self.input.runtime_intrinsic_for(key, &name.0)
    }

    pub(in crate::isle_adapter) fn scheduler_compiler_operation(
        &self,
        key: AstNodeKey,
    ) -> Option<crate::SchedulerCompilerOperation> {
        let name = self.query(runtime_intrinsic_name(self.db, key))?;
        self.input.scheduler_compiler_operation_for(key, &name.0)
    }

    pub(in crate::isle_adapter) fn collect_function_parameters(
        &self,
        key: AstNodeKey,
        parameters: &mut Vec<ParameterSlot>,
    ) -> Option<()> {
        let mut source_position =
            usize::from(self.query(node_kind(self.db, key)) == Some(beskid_queries::IndexedNodeKind::MethodDefinition));
        for child in self.raw_children(key) {
            match self.query(node_kind(self.db, child))? {
                beskid_queries::IndexedNodeKind::Block => continue,
                beskid_queries::IndexedNodeKind::Parameter => {
                    let identifier = self.raw_children(child).into_iter().find(|candidate| {
                        self.query(node_kind(self.db, *candidate)) == Some(beskid_queries::IndexedNodeKind::Identifier)
                    })?;
                    let slot = self.query(local_slot(self.db, identifier))?;
                    let specialization = self
                        .item_specializations
                        .get(&key)
                        .and_then(|specialization| specialization.signature.parameters.get(source_position))
                        .copied();
                    let semantic = specialization
                        .or_else(|| {
                            self.query(item_abi_signature(self.db, key))
                                .and_then(|signature| signature.parameters.get(source_position).copied())
                        })
                        .or_else(|| self.scalar_semantic_type(identifier))?;
                    source_position += 1;
                    if semantic == SemanticTypeId::UNIT {
                        continue;
                    }
                    let value_type = Some(semantic).and_then(|semantic| {
                        if matches!(semantic, SemanticTypeId::WORD | SemanticTypeId::POINTER | SemanticTypeId::STRING) {
                            self.isa.map(|isa| isa.pointer_type())
                        } else {
                            map_scalar_type(semantic)
                        }
                    })?;
                    let managed_reference =
                        if let Some(parameter_name) = self.query(parameter_generic_reference(self.db, child)) {
                            let binding = self
                                .item_specializations
                                .get(&key)?
                                .substitutions
                                .iter()
                                .find(|binding| binding.parameter.as_ref() == parameter_name.as_ref())?;
                            match binding.managed_reference_kind() {
                                ManagedReferenceKind::GcManaged => ManagedReferenceFact::GcManaged,
                                ManagedReferenceKind::NativeOrScalar => ManagedReferenceFact::NativeOrScalar,
                            }
                        } else {
                            self.managed_reference(identifier)?
                        };
                    parameters.push(ParameterSlot {
                        slot: LocalSlotId { owner_node: slot.owner.node.0, index: slot.index },
                        value_type,
                        managed_reference,
                    });
                }
                _ => self.collect_function_parameters(child, parameters)?,
            }
        }
        Some(())
    }
}
