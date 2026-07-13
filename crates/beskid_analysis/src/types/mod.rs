//! Structural type interning ([`TypeTable`]) and HIR type checking via [`TypeChecker`] against a [`crate::resolve::Resolution`].

pub mod checker;
pub mod display;
pub mod inference;
pub mod lowering_prep;
pub mod path_value;
pub mod result;
pub mod surface;
pub mod table;
pub mod try_desugar;

pub use checker::{CheckerResult, TryDesugarTarget, TypeChecker};
pub use display::format_type_id;
pub use inference::{
    Constraint, ConstraintSet, InferenceResult, TypeEnv, TypeVar,
    infer_generic_args_from_call_types, is_numeric, solve_constraints, unify_numeric_types,
    unify_types,
};
pub use lowering_prep::{CastIntent as LoweringCastIntent, LoweringPrep, LoweringPrepSurfaces};
pub use path_value::{
    PathTypeEnv, field_segments_before_method, field_type_for_value_path, field_type_on_receiver,
    first_field_segment_name, generic_mapping_for_type_id, method_name_from_path_callee,
    named_item_id, receiver_type_for_path_callee, resolve_path_base_local,
};
pub use result::{
    CallLoweringKind, FunctionSignature, MethodReceiverSource, TypeError, TypeResult, type_program,
    type_program_with_errors,
};
pub use surface::{
    MergedTypeEnv, UnitTypeSurface, build_unit_type_surface, contract_signatures_for_types,
    merge_unit_surfaces,
};
pub use table::{TypeId, TypeInfo, TypeTable};
pub use try_desugar::try_desugar_target_for_operand;
