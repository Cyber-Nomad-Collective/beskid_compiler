//! Runtime lowering policy for native IoC surface syntax.
//!
//! `launch` and `with` are validated during composition resolve in `beskid_analysis`.
//! Codegen treats them as no-ops until container/runtime lowering is implemented.
//! Unsupported targets must fail in analysis (for example `E1711` for `launch` in lib projects).

/// When false, lowering must not emit runtime container setup for `launch`/`with`.
pub const RUNTIME_CONTAINER_LOWERING_ENABLED: bool = false;
