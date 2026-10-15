pub use beskid_queries::AstNodeKey;

mod call_shapes;
mod catalogue;
mod destructive_read;
mod kinds;
mod match_range;
mod node_facts;
mod operations;

pub use call_shapes::{
    CallImportError, DirectCallee, FunctionItemValue, FunctionValueCall, InlineCaptureField, InlineClosureEnvironment,
    InlineLambdaCall, LambdaEntry,
    SpawnArgumentEnvironment, SpawnArgumentField, SpawnEntry, TracedFiberJoinLayout,
};
pub use catalogue::{
    NodeKind, SyntaxNodeClassification, UNSUPPORTED_TYPED_OPERATION_KINDS, classify_syntax_node_kind,
    syntax_node_kind_catalogue, unsupported_typed_operation_kinds,
};
pub use destructive_read::{CanonicalEmptyPlan, ExchangePlan, MoveExpressionPlan};
pub use kinds::{
    AssignmentKind, CallKind, EventOperation, ForIterableKind, IndexTarget, LiteralKind, OperatorFact,
    RuntimeIntrinsicKind,
};
pub use match_range::{MatchArmBindingFact, MatchArmFact, MatchPayloadPatternFact, RangeFact, Unit};
pub use node_facts::NodeFacts;
pub use operations::{
    ArrayForFact, CollectionMutationOwner, CollectionOperation, CompositionInjectionPlan, CompositionLaunchPlan,
    CompositionRegistrationPlan, CompositionScopePlan, EventHandlerLocalPlan, EventOperationPlan, IteratorForFact,
    LocalSlotId, ManagedReferenceFact, ParameterSlot, ScopedCleanupPlan,
};
