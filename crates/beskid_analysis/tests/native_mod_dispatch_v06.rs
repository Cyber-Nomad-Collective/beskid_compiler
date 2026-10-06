//! Missing executable Mod artifacts must never acquire successful contract outcomes.
use beskid_analysis::mod_host::{ContractInvoker, ContractRegistration, ModInvocationContext, NativeContractInvoker};

fn registration(kind: &str) -> ContractRegistration {
    ContractRegistration {
        contract_id: format!("Beskid.Compiler.Collect.{kind}"),
        type_id: format!("Fixture.{kind}"),
        entry_symbol: format!("fixture_{}", kind.to_ascii_lowercase()),
    }
}

#[test]
fn v06_missing_native_collector_cannot_return_stub_success() {
    let invoker = NativeContractInvoker::unavailable();
    let context = ModInvocationContext::empty();
    let result = invoker.invoke_collector(&registration("Collector"), &context.collect_request, None);
    assert!(result.is_err(), "missing native Collector was treated as success: {result:?}");
}

#[test]
fn v06_missing_native_analyzer_cannot_return_stub_success() {
    let invoker = NativeContractInvoker::unavailable();
    let context = ModInvocationContext::empty();
    let result = invoker.invoke_analyzer(&registration("Analyzer"), &context.collect_request, None, None);
    assert!(result.is_err(), "missing native Analyzer was treated as success: {result:?}");
}

#[test]
fn v06_missing_native_rewriter_cannot_return_stub_success() {
    let invoker = NativeContractInvoker::unavailable();
    let context = ModInvocationContext::empty();
    let result = invoker.invoke_rewriter(&registration("Rewriter"), &context.collect_request, None);
    assert!(result.is_err(), "missing native Rewriter was treated as success: {result:?}");
}

#[test]
fn v06_missing_native_generator_is_already_fail_closed() {
    let invoker = NativeContractInvoker::unavailable();
    let mut context = ModInvocationContext::empty();
    let request = context.generation_request(&[]);
    let result = invoker.invoke_generator(&registration("Generator"), &request, None);
    assert!(result.is_err(), "missing native Generator was treated as success: {result:?}");
}
