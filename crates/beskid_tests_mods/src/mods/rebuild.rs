//! End-to-end `beskid dev mod rebuild` descriptor extraction and host dispatch.

use std::fs;

use beskid_analysis::mod_host::NativeContractInvoker;

use beskid_tests_support::temp_case_dir;

#[test]
fn sample_mod_object_descriptor_cannot_qualify_executable_dispatch() {
    let root = temp_case_dir("sample_mod_object_rejected");
    fs::create_dir_all(&root).unwrap();
    let object = root.join("legacy-mod.o");
    fs::write(&object, b"source-only relocatable object").unwrap();
    let invoker = NativeContractInvoker::unavailable();
    let registration = beskid_analysis::mod_host::ContractRegistration {
        contract_id: "Beskid.Compiler.Collect.Collector".into(),
        type_id: "SampleMod.SampleCollect".into(),
        entry_symbol: "samplemod_collect".into(),
    };
    let context = beskid_analysis::mod_host::ModInvocationContext::empty();
    let error =
        beskid_analysis::mod_host::ContractInvoker::invoke_collector(&invoker, &registration, &context.collect_request, None)
            .unwrap_err();
    assert!(error.message.contains("producer-issued executable authority"), "{error}");
    let _ = fs::remove_dir_all(&root);
}
