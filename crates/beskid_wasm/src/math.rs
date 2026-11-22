use anyhow::{Result, ensure};
use beskid_codegen::ExternImport;
use cranelift_codegen::{
    ir::{AbiParam, Signature, types::F64},
    isa::CallConv,
};

/// Browser ABI v1 binds this subset of the C math library to JavaScript Math.
pub(crate) fn is_host_import(name: &str, signature: &Signature, externs: &[ExternImport]) -> Result<bool> {
    let arity = match name {
        "log" | "log2" | "log10" | "sin" | "cos" | "tan" => 1,
        "atan2" => 2,
        _ => return Ok(false),
    };
    ensure!(
        externs.iter().filter(|external| external.symbol == name).all(|external| {
            external.abi.as_deref().is_none_or(|abi| abi == "C")
                && external.library.as_deref().is_none_or(|library| library == "m")
                && !external.optional
        }),
        "browser libm binding {name} requires a non-optional C import from the default library or m"
    );
    let float = AbiParam::new(F64);
    ensure!(
        signature.call_conv == CallConv::SystemV
            && signature.params.len() == arity
            && signature.params.iter().all(|param| *param == float)
            && signature.returns == [float],
        "browser libm binding {name} requires {arity} f64 argument(s) and an f64 result"
    );
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cranelift_codegen::ir::types::I32;

    #[test]
    fn libm_binding_requires_its_exact_signature_and_library() {
        let float = AbiParam::new(F64);
        let mut signature = Signature::new(CallConv::SystemV);
        signature.params.push(float);
        signature.returns.push(float);
        let mut import =
            ExternImport { symbol: "log".into(), abi: Some("C".into()), library: Some("m".into()), optional: false };
        assert!(is_host_import("log", &signature, std::slice::from_ref(&import)).unwrap());
        assert!(!is_host_import("sqrt", &signature, &[]).unwrap());
        assert!(is_host_import("atan2", &signature, &[]).is_err());
        import.library = Some("other".into());
        assert!(is_host_import("log", &signature, &[import]).is_err());
        signature.params[0] = AbiParam::new(I32);
        assert!(is_host_import("log", &signature, &[]).is_err());
    }
}
