use std::collections::HashMap;
use std::path::PathBuf;

use crate::resolve::{AstNodeId, ItemId, LocalId};
use crate::syntax::SpanInfo;
use crate::types::path_value::PathTypeEnv;
use crate::types::result::{CallLoweringKind, FunctionSignature};
use crate::types::{TypeId, TypeTable};

/// Cast intent keyed by syntax node id (span retained for diagnostics).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CastIntent {
    pub node_id: AstNodeId,
    pub span: SpanInfo,
    pub from: TypeId,
    pub to: TypeId,
    pub source_path: Option<PathBuf>,
}

/// Call dispatch and cast metadata for codegen lowering.
#[derive(Debug, Default, Clone)]
pub struct LoweringPrep {
    pub call_kinds: HashMap<AstNodeId, CallLoweringKind>,
    pub cast_intents: Vec<CastIntent>,
}

/// Read-only type surface inputs for lowering prep (orchestrator merges unit surfaces).
pub struct LoweringPrepSurfaces<'a> {
    pub types: &'a TypeTable,
    pub local_types: &'a HashMap<LocalId, TypeId>,
    pub function_signatures: &'a HashMap<ItemId, FunctionSignature>,
    pub method_function_signatures: &'a HashMap<ItemId, FunctionSignature>,
    pub struct_fields_ordered: &'a HashMap<ItemId, Vec<(String, TypeId)>>,
    pub struct_event_fields: &'a HashMap<ItemId, HashMap<String, Option<usize>>>,
    pub enum_variants_ordered: &'a HashMap<ItemId, Vec<(String, Vec<TypeId>)>>,
    pub generic_items: &'a HashMap<ItemId, Vec<String>>,
    pub methods_by_receiver: &'a HashMap<(ItemId, String), ItemId>,
    pub contract_signatures: &'a HashMap<(ItemId, String), FunctionSignature>,
    pub named_types: &'a HashMap<ItemId, TypeId>,
}

impl<'a> LoweringPrepSurfaces<'a> {
    pub fn path_env(&self) -> PathTypeEnv<'a> {
        PathTypeEnv {
            types: self.types,
            local_types: self.local_types,
            struct_fields_ordered: self.struct_fields_ordered,
            generic_items: self.generic_items,
        }
    }
}

impl LoweringPrep {
    pub fn call_kind_at(&self, node_id: AstNodeId) -> Option<&CallLoweringKind> {
        self.call_kinds.get(&node_id)
    }

    pub fn cast_intents_for_node(&self, node_id: AstNodeId) -> impl Iterator<Item = &CastIntent> {
        self.cast_intents.iter().filter(move |intent| intent.node_id == node_id)
    }
}
