//! Runtime lowering policy for native IoC surface syntax.
//!
//! `launch` and `with` are validated during composition resolve in `beskid_analysis`.
//! When this gate is `true`, codegen lowers them through the
//! [`crate::lowering::composition`] module into the runtime container ABI
//! (`composition_container_create` / `composition_launch` / `composition_scope_enter` /
//! `composition_scope_leave` / `composition_shutdown` / `composition_container_drop`).
//!
//! Unsupported targets must still fail in analysis (for example `E1711` for `launch` in
//! lib projects); the runtime container can be queried for active state but the analysis
//! diagnostics remain authoritative.

/// Flip to enable lowering of `launch` and `with` to runtime container ABI calls.
pub const RUNTIME_CONTAINER_LOWERING_ENABLED: bool = true;
