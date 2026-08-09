use crate::errors::CodegenError;
use crate::linking::resolve_item_call_id;
use crate::lowering::cast_intent::ensure_type_compatibility_or_expected;
use crate::lowering::dispatch::lower_dispatch_builtin_call;
use crate::lowering::function::{
    lower_function_with_name, mangle_generic_item_function, mangle_item_function, mangle_method_name,
};
use crate::lowering::locals::{call_kind_for_call, canonicalize_call_kind, local_id_for_span, resolved_value_at};
use crate::lowering::lowerable::{Lowerable, lower_node};
use crate::lowering::node_context::NodeLoweringContext;
use crate::lowering::type_surface::{contract_method_order, contract_signatures};
use crate::lowering::types::{map_type_id_to_clif, pointer_type};
use beskid_abi::{DispatchReturnGroup, DispatchRoute, TAG_EVENT_GET_HANDLER, TAG_EVENT_LEN, dispatch_route_for_symbol};
use beskid_analysis::builtins::{BuiltinType, builtin_specs};
use beskid_analysis::hir::{HirCallExpression, HirExpressionNode, HirLambdaExpression, HirPrimitiveType};
use beskid_analysis::resolve::{ItemKind, ResolvedValue, canonical_item_id};
use beskid_analysis::syntax::{SpanInfo, Spanned};
use beskid_analysis::types::{
    CallLoweringKind, MethodReceiverSource, TypeId, TypeInfo, first_field_segment_name, method_name_from_path_callee,
    resolve_path_base_local,
};
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{
    AbiParam, ExtFuncData, ExternalName, Function, InstBuilder, MemFlags, Signature, TrapCode, Value, types,
};
use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings;
use cranelift_codegen::verify_function;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use std::collections::HashMap;

mod common;
mod contract_event;
mod event;
mod generics;
mod indirect;
mod lambda;
mod lower;
mod method;

pub(crate) use self::common::type_returns_runtime_value;
pub(crate) use self::lambda::{lower_lambda_function_value, lower_spawn_lambda_target};
