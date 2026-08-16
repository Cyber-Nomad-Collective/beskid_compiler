use cranelift_codegen::ir::{Type, types};

pub fn pointer_type() -> Type {
    if cfg!(target_pointer_width = "64") { types::I64 } else { types::I32 }
}
