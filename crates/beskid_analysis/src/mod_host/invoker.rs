//! Contract invocation abstraction for `mod.collect` / `mod.generate` / `mod.analyze` /
//! `mod.rewrite` phases.
//!
//! The host calls one of [`ContractInvoker`]'s methods for every scheduled
//! `(contractId, typeId, entrySymbol)` tuple discovered by `mod.load`. Implementations
//! decide how to reach the Beskid-side contract instance — current implementations are:
//!
//! * [`StubContractInvoker`] and [`ScriptedContractInvoker`] are explicit test helpers.
//! * [`NativeContractInvoker`](super::native::NativeContractInvoker) owns native libraries.
//!
//! Semantic authority is borrowed separately for each synchronous invocation. It is never
//! stored in the library owner or transferred across threads.

mod contract;
mod outcomes;
mod scripted;
mod stub;

#[cfg(test)]
mod tests;

pub use contract::{ContractInvocationError, ContractInvoker};
pub use outcomes::{
    AnalyzerDiagnostic, AnalyzerFix, AnalyzerOutcome, AnalyzerSeverity, CollectorOutcome, GeneratorOutcome,
    RewriteEdit, RewriterOutcome,
};
pub use scripted::ScriptedContractInvoker;
pub use stub::{InvocationKind, StubContractInvoker};
