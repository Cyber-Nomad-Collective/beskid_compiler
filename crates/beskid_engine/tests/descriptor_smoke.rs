use anyhow::Result;
use beskid_codegen::services::lower_source;
use beskid_engine::Engine;
use cranelift_module::{FuncOrDataId, Module};

#[test]
fn descriptor_symbol_exists_for_named_type() -> Result<()> {
    // Minimal program with a named type to trigger descriptor emission.
    let src = r#"
pub type Point { i64 x, i64 y }

pub i64 main() { return 0; }
"#;
    let lowered = lower_source(std::path::Path::new("<memory>"), src, false)?;
    assert!(!lowered.artifact.type_descriptors.is_empty());

    // Pick one descriptor and compute the expected data symbol name.
    let (type_id, _) = lowered.artifact.type_descriptors.iter().next().unwrap();
    let symbol = format!("__beskid_type_desc_{}", type_id.0);

    let mut engine = Engine::new();
    engine
        .compile_artifact(&lowered.artifact)
        .expect("compile artifact");
    let module = engine.jit_module_mut();

    // The data symbol must be present in the JIT module namespace.
    let id = module.get_name(&symbol).expect("descriptor symbol present");
    match id {
        FuncOrDataId::Func(_) => panic!("expected data id for descriptor symbol"),
        FuncOrDataId::Data(_) => {}
    }
    Ok(())
}
