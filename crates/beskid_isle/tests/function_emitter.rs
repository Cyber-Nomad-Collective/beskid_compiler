use std::str::FromStr;

use beskid_isle::FunctionEmitter;
use cranelift_codegen::settings;
use target_lexicon::Triple;

#[test]
fn signatures_and_pointer_types_come_from_each_supported_isa() {
    for triple in [
        "x86_64-unknown-linux-gnu",
        "aarch64-apple-darwin",
        "x86_64-pc-windows-msvc",
    ] {
        let triple = Triple::from_str(triple).expect("supported triple syntax");
        let isa = cranelift_codegen::isa::lookup(triple.clone())
            .unwrap_or_else(|error| panic!("lookup {triple}: {error}"))
            .finish(settings::Flags::new(settings::builder()))
            .unwrap_or_else(|error| panic!("finish {triple}: {error}"));
        let emitter = FunctionEmitter::new(isa.as_ref());
        let signature = emitter.signature([], []);

        assert_eq!(signature.call_conv, isa.default_call_conv(), "{triple}");
        assert_eq!(emitter.pointer_type(), isa.pointer_type(), "{triple}");
    }
}
