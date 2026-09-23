//! Contract invocation abstraction for `mod.collect` / `mod.generate` / `mod.analyze` /
//! `mod.rewrite` phases.
//!
//! The host calls one of [`ContractInvoker`]'s methods for every scheduled
//! `(contractId, typeId, entrySymbol)` tuple discovered by `mod.load`. Implementations
//! decide how to reach the Beskid-side contract instance — current implementations are:
//!
//! * [`StubContractInvoker`] — default for tests and pre-AOT bring-up. Records
//!   invocations and returns empty results so `mod.collect`-`mod.rewrite` complete
//!   deterministically.
//! * [`ScriptedContractInvoker`] — test helper that scripts per-`typeId` outcomes for
//!   assertions in beskid_engine and beskid_tests.
//! * [`NativeContractInvoker`](super::native::NativeContractInvoker) — records artifact
//!   object paths and delegates to a stub until shared-library dlopen dispatch lands.
//!
//! Future implementations will dlopen the AOT object and
//! call `entry_symbol` with the Beskid → C ABI defined in `beskid_abi`. The trait
//! shape stays stable so production code can switch implementations without touching
//! `mod_host` orchestration.

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
