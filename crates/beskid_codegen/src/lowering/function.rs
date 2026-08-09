mod body_emission;
mod entrypoints;
mod generics;
mod mangling;
mod method_test;
mod return_types;

pub(crate) use body_emission::{FunctionLoweringState, LoopControl, refresh_locals_at_loop_header};
pub(crate) use entrypoints::{lower_function, lower_function_with_name};
pub(crate) use generics::{
    generic_mapping_for_method_receiver, generic_mapping_from_mangled, is_self_parameter_function,
};
pub(crate) use mangling::{
    linker_name_for_item_function, mangle_generic_item_function, mangle_item_function, mangle_method_name,
};
pub(crate) use method_test::{lower_method, lower_test};
pub(crate) use return_types::item_id_for_item_span;
