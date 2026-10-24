//! Item, local, slot, and mutable-assignment resolution facts.

use beskid_abi::{abi_v5::AbiType, runtime_source::RuntimeIntrinsicCapability};
use beskid_analysis::projects::ProgramAssembly;
use beskid_analysis::syntax::SyntaxGenerationId;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::db::Db;
use crate::inputs::ProjectSession;

use super::super::queries::{node_kind, node_span};
use super::*;

/// Resolution fact for an item reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ResolvedItem {
    pub declaration: AstNodeKey,
}

/// Resolution fact for a local reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ResolvedLocal {
    pub declaration: AstNodeKey,
}

/// Owner-qualified backend slot for an exact local declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct LocalSlot {
    pub owner: AstNodeKey,
    pub index: u32,
}

/// A current-generation mutable local write target proven from syntax alone.
///
/// The fact exists only for a single-segment path that resolves to a mutable `let` binding or
/// mutable function/method parameter. Immutable, non-local, stale, and invalid targets remain
/// unavailable, so ISLE cannot manufacture a write from a bare local slot. A compound assignment
/// (`total += 2`) carries its operator: lowering reads the local once, applies the operator with
/// the same rules as the binary expression, and writes the result back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct MutableLocalAssignment {
    pub declaration: AstNodeKey,
    pub slot: LocalSlot,
    pub compound: Option<CompoundAssignmentOperator>,
}

/// The arithmetic operator of a compound assignment `a op= b`, lowered as `a = a op b`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CompoundAssignmentOperator {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

impl CompoundAssignmentOperator {
    /// The operator of `op`, or `None` for a plain `=`. Every compound assignment operator of the
    /// grammar maps here; never add a wildcard arm (it would lower `*=` as `=`).
    pub fn of(op: beskid_analysis::syntax::AssignOp) -> Option<Self> {
        match op {
            beskid_analysis::syntax::AssignOp::Assign => None,
            beskid_analysis::syntax::AssignOp::AddAssign => Some(Self::Add),
            beskid_analysis::syntax::AssignOp::SubAssign => Some(Self::Sub),
            beskid_analysis::syntax::AssignOp::MulAssign => Some(Self::Mul),
            beskid_analysis::syntax::AssignOp::DivAssign => Some(Self::Div),
            beskid_analysis::syntax::AssignOp::RemAssign => Some(Self::Rem),
        }
    }
}
