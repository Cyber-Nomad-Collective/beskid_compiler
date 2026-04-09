#![cfg(target_os = "linux")]

use anyhow::Result;
use beskid_codegen::{CodegenArtifact, ExternImport, LoweredFunction};
use beskid_engine::Engine;
use cranelift_codegen::ir::{AbiParam, ExternalName, Function, Signature, types};
use cranelift_codegen::isa::CallConv;

#[test]
#[cfg(feature = "extern_dlopen")]
fn extern_signature_validation_rejects_disallowed_types() -> Result<()> {
    // Build a function that imports an extern symbol with a disallowed param type (i16),
    // without even calling it; the JIT validator scans ext_funcs and rejects the signature.
    let mut func = Function::new();
    func.signature = {
        let mut s = Signature::new(CallConv::SystemV);
        s.returns.push(AbiParam::new(types::I64));
        s
    };

    // Import bad extern signature: getpid(i16) -> i64 (i16 is not in the allowed FFI kinds)
    let mut bad = Signature::new(CallConv::SystemV);
    bad.params.push(AbiParam::new(types::I16));
    bad.returns.push(AbiParam::new(types::I64));
    let bad_sig = func.import_signature(bad);
    let _ = func.import_function(cranelift_codegen::ir::ExtFuncData {
        name: ExternalName::testcase("getpid"),
        signature: bad_sig,
        colocated: true,
        patchable: false,
    });

    // Artifact carries extern import for the same symbol so the engine resolves it.
    let artifact = CodegenArtifact {
        functions: vec![LoweredFunction {
            name: "main".into(),
            function: func,
        }],
        type_descriptors: Default::default(),
        string_literals: Default::default(),
        extern_imports: vec![ExternImport {
            symbol: "getpid".into(),
            abi: Some("C".into()),
            library: Some("libc.so.6".into()),
        }],
    };

    let mut engine = Engine::new();
    let err = engine
        .compile_artifact(&artifact)
        .expect_err("should fail FFI signature validation");
    let msg = format!("{:?}", err);
    assert!(msg.contains("extern signature not allowed for getpid"));
    assert!(msg.contains("param type"));
    Ok(())
}
