//! Blank-line and spacing policy (C# / CSharpier-style), adapted from bsharp `emitters/policy.rs`.

use crate::format::emit::{EmitCtx, EmitError};
use crate::syntax::Statement;
use std::fmt::Write;

pub(crate) fn between_top_level_declarations<W: Write>(
    cx: &mut EmitCtx,
    w: &mut W,
) -> Result<(), EmitError> {
    cx.nl(w)
}

pub(crate) fn between_members<W: Write>(cx: &mut EmitCtx, w: &mut W) -> Result<(), EmitError> {
    cx.nl(w)
}

/// Extra separator when a control-flow construct is followed by a `let` in the same block.
pub(crate) fn between_block_items<W: Write>(
    cx: &mut EmitCtx,
    w: &mut W,
    prev: &Statement,
    next: &Statement,
) -> Result<(), EmitError> {
    let prev_is_block_like = matches!(
        prev,
        Statement::If(_) | Statement::While(_) | Statement::For(_)
    );
    if prev_is_block_like && matches!(next, Statement::Let(_)) {
        return cx.nl(w);
    }
    Ok(())
}
