pub use beskid_queries::AstNodeKey;

mod call_shapes;
mod catalogue;
mod kinds;
mod match_range;
mod node_facts;
mod operations;

pub use call_shapes::{
    CallImportError, DirectCallee, InlineCaptureField, InlineClosureEnvironment, InlineLambdaCall, LambdaEntry,
    SpawnArgumentEnvironment, SpawnArgumentField, SpawnEntry, TracedFiberJoinLayout,
};
pub use catalogue::{
    NodeKind, SyntaxNodeClassification, UNSUPPORTED_TYPED_OPERATION_KINDS, classify_syntax_node_kind,
    syntax_node_kind_catalogue, unsupported_typed_operation_kinds,
};
pub use kinds::{CallKind, ForIterableKind, IndexTarget, LiteralKind, OperatorFact, RuntimeIntrinsicKind};
pub use match_range::{MatchArmBindingFact, MatchArmFact, MatchPayloadPatternFact, RangeFact, Unit};
pub use node_facts::NodeFacts;
pub use operations::{
    CollectionMutationOwner, CollectionOperation, LocalSlotId, ManagedReferenceFact, ParameterSlot, ScopedCleanupPlan,
};
