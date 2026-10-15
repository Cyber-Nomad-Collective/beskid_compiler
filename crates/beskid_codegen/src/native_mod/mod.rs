//! Compiler-issued native SDK adapter plans and canonical marshaling correspondence.
pub mod sdk_schema;
pub mod sdk_sources;

pub(crate) mod callbacks;

mod contracts;

mod authority;
mod requests;

mod marshaling;

pub(crate) mod adapter_plan;
mod transport_layout;
mod constructors;
pub(crate) mod producer;
mod data_getters;
mod c_codec;
