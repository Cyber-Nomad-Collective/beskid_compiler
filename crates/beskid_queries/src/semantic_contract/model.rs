//! Public AST/Salsa semantic contracts used by later frontend and codegen replacement slices.

pub use beskid_abi::runtime_source::CorelibService;

mod closures;
mod errors;
mod facts;
mod generics;
mod ids;
mod layouts;
mod resolution;
mod syntax;
mod typed_program;
mod types;

pub use closures::{
    CaptureStorage, CaptureStorageClass, ClosureAllocationStatus, ClosureCallTarget, ClosureCapture,
    ClosureEnvironment, ClosureEnvironmentAbiShape, ClosureEnvironmentField, ClosureLoweringStatus,
    ClosurePointerMapRequirement, ClosureSignature, FiberOwnership, SpawnDiagnostic, SpawnDiagnosticKind,
    SpawnEntryValidation, SpawnHandleType, SpawnLegality, SpawnTarget,
};
pub use errors::{SemanticError, SemanticFinding, SemanticQueryResult};
pub use facts::{
    BulkParameterFact, CastIntent, ControlFlow, ForIteratorFact, ItemSignature, PrimitiveNumericConversion,
    RangeForFact, TryExpressionFact,
};
pub(in crate::semantic_contract) use generics::GenericSourceTypeIdentity;
pub use generics::{
    ArrayIndexElementTemplate, ContractParameterWitness, GenericCallInstantiation, GenericCallSpecialization,
    GenericCallTemplate, GenericNominalMethodReceiver, GenericSpecializationInstance, GenericSubstitution,
    generic_specialization_identity,
};
pub use ids::{
    AstNodeKey, SourceUnitId, format_ast_node_key, format_ast_node_site, format_ast_node_trace,
    format_source_span_range,
};
pub use layouts::{
    AggregateFieldAccess, AggregateFieldShape, AggregateLayoutFact, AggregateLiteralFieldValues, EnumConstructorFact,
    EnumConstructorSpecialization, EnumConstructorTemplate, EnumLayoutFact, EnumLayoutTemplateArgument,
    EnumMatchArmFact, EnumMatchBindingFact, EnumMatchFact, EnumMatchPatternFact, EnumMatchScalarLiteralFact,
    EnumMatchVariantPatternFact, EnumScalarPayloadObjectLayout, EnumScalarPayloadVariantLayout, EnumVariantLayoutFact,
    ScalarAbiLayout,
};
pub use resolution::{LocalSlot, MutableLocalAssignment, ResolvedItem, ResolvedLocal};
pub use syntax::{
    CompletionCandidate, CompletionContext, CompletionKind, CompletionMemberSurface, ExportSymbol, IndexedNodeKind,
    LiteralFact, OperatorFact, RuntimeIntrinsic, RuntimeIntrinsicName, SourceSpan, TestItem,
};
pub use typed_program::{SyntaxUnitInput, SyntaxUnitRevision, TypedProgram};
pub use types::{
    CallLowering, CollectionMutationOwner, CollectionOperation, ManagedReferenceKind, ManifestBuiltin, SemanticTypeId,
    TypedArrayAllocation,
};
