//! Per-type Beskid (`.bd`) emission for the syntax AST vocabulary in `beskid_analysis`.
//!
//! Scans `syntax/{items,types,expressions,statements,common}/**/*.rs`, emits one file per
//! `pub struct` / `pub enum` (skipping `mod.rs` and helpers). Rust generic parameters map to
//! `ReflectStub` in generated fields; leading docs emit matching `@par` lines.
//!
//! Leading `///` bodies use the structured doc subset from `beskid_analysis` / `beskid_doc.pest`
//! (`@variant`, `@par`, and plain `Run` text). Mirrored field `///` lines are taken from Rust
//! `#[doc = ...]` only; `@arg` / `@returns` stay for hand-authored callables.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use syn::{
    Attribute, Expr, Fields, GenericArgument, GenericParam, Generics, Item, Lit, Meta, PathArguments, Type, Visibility,
};

use crate::emit_idents::rust_snake_to_beskid_field_camel;
use crate::syntax_helpers::{
    self, HelperPaths, list_element_rust_name, option_payload_rust_name, peel_type, vec_element_type,
};

pub use crate::syntax_helpers::{
    SYNTAX_NODES_MODULE_PREFIX, SYNTAX_SCAN_SKIP_FILES, SYNTAX_SCAN_SUBDIRS, reflect_stub_path,
};

mod docs_naming;
mod emit;
mod inventory;
mod model;
mod reflect;
mod type_mapping;

#[cfg(test)]
mod tests;

pub use self::emit::emit_syntax_sdk;
pub use self::inventory::inventory_syntax_type_names;
pub(crate) use self::model::BANNER;
pub use self::model::{EnumVariantMirror, FieldMirror, ParsedType, SyntaxNodesGenReport, TypeKind, VariantShape};
pub use self::reflect::reflect_sdk_node_kind_names;
