//! [`BeskidJitModule`]: symbol declaration, compilation, and native JIT ISA policy.

mod declare;
mod errors;
mod isa;
mod module;

#[cfg(all(test, target_arch = "x86_64", any(unix, windows)))]
mod native_jit_policy_tests;

pub use errors::JitError;
pub use module::BeskidJitModule;

pub(crate) use isa::native_jit_isa;
