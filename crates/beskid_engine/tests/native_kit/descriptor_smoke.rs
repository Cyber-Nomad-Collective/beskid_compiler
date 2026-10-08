use std::path::Path;

use anyhow::Result;
use beskid_abi::runtime_kit::BuildProfile;
use beskid_engine::services::prepare_jit_entrypoint;
use beskid_engine::{Engine, host_runtime_target};

use crate::shared_kit;
use cranelift_module::{FuncOrDataId, Module};

#[test]
fn codegen_input_artifact_compiles_and_exposes_entrypoint_symbol() -> Result<()> {
    // Named-type descriptor payloads are not yet emitted on the sole CodegenInput → ISLE
    // route; this smoke proves the JIT consumer accepts that route under an exact kit.
    let kit_lease = shared_kit::debug();
    let prefix = kit_lease.prefix();
    let target = host_runtime_target().expect("host target");
    let mut engine = Engine::with_runtime_kit(prefix, target, BuildProfile::Debug).expect("load exact kit");

    let prepared = prepare_jit_entrypoint(Path::new("<memory>"), "pub i64 Main() { return 0; }", "Main")?;
    assert!(prepared.symbol.starts_with("Main#syntax_"));

    engine.compile_artifact(&prepared.artifact).expect("compile CodegenInput artifact");
    let module = engine.jit_module_mut();
    let id = module.get_name(&prepared.symbol).expect("entrypoint symbol present");
    match id {
        FuncOrDataId::Func(_) => {}
        FuncOrDataId::Data(_) => panic!("expected func id for entrypoint symbol"),
    }
    Ok(())
}
