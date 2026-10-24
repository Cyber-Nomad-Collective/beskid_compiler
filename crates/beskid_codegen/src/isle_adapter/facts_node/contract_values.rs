//! Contract values (runtime contract dispatch, v0.6.7): box coercions, witness dispatch and
//! unboxed receivers, answered from the semantic queries in the specialization being lowered.

use beskid_isle::{ContractBoxPlan, ContractBoxReceiverFact, ContractDispatchCall};
use beskid_queries::{AstNodeKey, ContractBoxReceiver, ContractWitnessTable, SemanticTypeId};

use super::SyntaxNodeFacts;
use crate::contract_static::{CONTRACT_BOX_REFERENCE_REQUEST_SYMBOL, CONTRACT_BOX_SCALAR_REQUEST_SYMBOL, scalar_receiver_type};
use crate::isle_adapter::mappings::signature_for_item;

impl SyntaxNodeFacts<'_> {
    /// The box the concrete value at `key` is written into a contract-typed destination with.
    pub(super) fn contract_box_impl(&self, key: AstNodeKey) -> Option<ContractBoxPlan> {
        let coercion =
            self.query(beskid_queries::contract_box_coercion(self.db, key, self.current_item_specialization()))?;
        self.contract_box_plan(&coercion.witness)
    }

    /// The box plan of one witness table.
    pub(in crate::isle_adapter) fn contract_box_plan(&self, table: &ContractWitnessTable) -> Option<ContractBoxPlan> {
        let (allocation_request_symbol, receiver) = match table.receiver {
            ContractBoxReceiver::Reference => (CONTRACT_BOX_REFERENCE_REQUEST_SYMBOL, ContractBoxReceiverFact::Reference),
            ContractBoxReceiver::Scalar(SemanticTypeId::STRING) => {
                (CONTRACT_BOX_REFERENCE_REQUEST_SYMBOL, ContractBoxReceiverFact::Reference)
            }
            ContractBoxReceiver::Scalar(primitive) => {
                (CONTRACT_BOX_SCALAR_REQUEST_SYMBOL, ContractBoxReceiverFact::Scalar(scalar_receiver_type(primitive)?))
            }
        };
        Some(ContractBoxPlan {
            allocation_request_symbol: allocation_request_symbol.into(),
            witness_symbol: table.symbol.clone(),
            receiver,
        })
    }

    /// The witness-table dispatch of the contract method call at `key`, when no static target
    /// (a specialization witness, conf2's local or injected-field selection) applies.
    pub(super) fn contract_dispatch_impl(&self, key: AstNodeKey) -> Option<ContractDispatchCall> {
        let dispatch = self.query(beskid_queries::contract_dispatch(self.db, key, self.current_item_specialization()))?;
        // An injected field bound to one implementation keeps conf2's direct call.
        if self.injected_contract_implementation(key, dispatch.signature_node).is_some() {
            return None;
        }
        let mut signature = signature_for_item(self.isa?, dispatch.signature)?;
        signature.params.first_mut()?.value_type = self.isa?.pointer_type();
        Some(ContractDispatchCall { slot: dispatch.slot, signature })
    }

    /// Whether the direct method call at `key` reads its receiver word out of a contract box.
    pub(super) fn contract_value_receiver_impl(&self, key: AstNodeKey) -> bool {
        let Some(beskid_queries::CallLowering::Direct(declaration)) = self.query(beskid_queries::call_lowering(self.db, key))
        else {
            return false;
        };
        if self.query(beskid_queries::node_kind(self.db, declaration)) != Some(beskid_queries::IndexedNodeKind::MethodDefinition)
        {
            return false;
        }
        self.query(beskid_queries::contract_value_receiver(self.db, key, self.current_item_specialization()))
            .unwrap_or(false)
    }
}
