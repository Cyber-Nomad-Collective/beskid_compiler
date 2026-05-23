//! Lowering for the language-meta composition surface (`launch` / `with`).
//!
//! Codegen turns each `launch <Host>` statement into a paired
//! `composition_container_create` → `composition_launch` → `composition_shutdown` →
//! `composition_container_drop` sequence around an optional body, and each
//! `with <scope>` statement into `composition_scope_enter` /
//! `composition_scope_leave` brackets.
//!
//! Field `inject` (single + plural) and ctor wiring is left to a later HIR pass:
//! the runtime container already exposes `composition_resolve` /
//! `composition_resolve_plural` and `composition_bind_plural`, so the lowering can grow
//! into those entry points without further runtime/ABI churn.

pub mod launch_statement;
pub mod with_statement;

pub(crate) use launch_statement::lower_launch_statement;
pub(crate) use with_statement::lower_with_statement;
