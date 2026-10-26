//! House style: brace placement, condition parentheses and blank lines between elements.
//!
//! The style follows .NET (C#) formatting conventions, as ruled for v0.6.7:
//! - Allman braces: every declaration and statement body opens with `{` on its own line.
//!   Blocks in expression position (lambda bodies, block expressions, macro invocation blocks)
//!   stay on the line of their expression.
//! - Conditions are parenthesized: `if (x)`, `while (x)`, `match (x)`.
//! - Blank lines: consecutive members of one kind (`use` lines, `mod` lines, constants, fields,
//!   enum variants, contract members, entries, statements, arms) stay together unless the author
//!   separated them; one blank line separates items of different kinds, type declarations,
//!   functions and methods. A run of author blank lines collapses to one, and no blank line
//!   follows an opening brace.
//!
//! Brace placement and condition parentheses are each one switch here.

use crate::format::emit::{EmitCtx, EmitError};
use crate::syntax::Node;
use std::fmt::Write;

/// Allman braces: the opening brace of a declaration or statement body goes on its own line.
const BRACE_ON_OWN_LINE: bool = true;

/// Conditions of `if`, `while` and `match` are written in parentheses.
pub(crate) const PARENTHESIZE_CONDITIONS: bool = true;

/// Write what separates a header from the `{` of its body. After a `//` comment on the header
/// line the brace always starts a new line.
pub(crate) fn before_open_brace<W: Write>(
    cx: &mut EmitCtx,
    w: &mut W,
    after_line_comment: bool,
) -> Result<(), EmitError> {
    if BRACE_ON_OWN_LINE || after_line_comment {
        cx.nl(w)?;
        cx.write_indent(w)
    } else {
        cx.space(w)
    }
}

/// Item kinds for blank-line separation between top-level (and inline-module) items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ItemKind {
    Use,
    ModuleDeclaration,
    Constant,
    /// Every other item: types, functions, tests, impls, hosts, macros, inline modules.
    Declaration,
}

pub(crate) fn item_kind(node: &Node) -> ItemKind {
    match node {
        Node::UseDeclaration(_) => ItemKind::Use,
        Node::ModuleDeclaration(_) => ItemKind::ModuleDeclaration,
        Node::ConstantDefinition(_) => ItemKind::Constant,
        _ => ItemKind::Declaration,
    }
}

/// One blank line between two items unless both are consecutive lines of one compact kind.
pub(crate) fn blank_between_items(previous: Option<ItemKind>, next: ItemKind) -> bool {
    match previous {
        None => false,
        Some(previous) => previous != next || next == ItemKind::Declaration,
    }
}
