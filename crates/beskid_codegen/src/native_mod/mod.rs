//! Compiler-issued native SDK adapter plans and canonical marshaling correspondence.
pub mod sdk_schema;
pub mod sdk_sources;

pub(crate) mod callbacks;

mod contracts;

mod authority;
mod requests;

mod marshaling;

pub(crate) mod adapter_plan;
mod c_codec;
mod constructors;
mod data_getters;
pub(crate) mod producer;
mod transport_layout;
