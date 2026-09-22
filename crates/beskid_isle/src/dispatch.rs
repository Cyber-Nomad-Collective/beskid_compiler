use cranelift_codegen::ir::Type;
use cranelift_codegen::isa::TargetFrontendConfig;

pub fn pointer_type(frontend_config: TargetFrontendConfig) -> Type {
    frontend_config.pointer_type()
}
