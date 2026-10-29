use std::sync::Arc;

use cranelift_codegen::{
    isa::{self, TargetIsa},
    settings::{self, Configurable},
};
use cranelift_jit::JITBuilder;
use cranelift_module::default_libcall_names;
use target_lexicon::Architecture;

use super::errors::JitError;

pub(super) fn new_builder(extras: &[(String, *const u8)]) -> Result<JITBuilder, JitError> {
    let isa = native_jit_isa()?;
    let mut builder = JITBuilder::with_isa(isa, default_libcall_names());
    for (sym, addr) in extras {
        builder.symbol(sym, *addr);
    }
    Ok(builder)
}

/// Construct the native ISA shared by JIT lowering and final emission.
///
/// The ISA has the host target baseline (`beskid_codegen::target_isa`, x86-64-v2 on x86_64), the same as an
/// AOT build for the host target; host CPU detection never adds a feature. A host CPU below the baseline
/// fails here, before any code is generated.
///
/// This is the sole owner of JIT-only relocation and optimization policy. The shared codegen
/// settings retain frame pointers for the tail-call invariant; JIT then selects x86_64 PIC from
/// the target ISA, disables colocated libcalls for every native target, and always optimizes
/// (`opt_level=speed`): the JIT emits no debug info that unoptimized code would preserve.
pub(crate) fn native_jit_isa() -> Result<Arc<dyn TargetIsa>, JitError> {
    let builder = beskid_codegen::target_isa::host_isa_builder().map_err(|error| JitError::Isa(error.to_string()))?;
    native_jit_isa_from_builder(builder)
}

pub(super) fn native_jit_isa_from_builder(builder: isa::Builder) -> Result<Arc<dyn TargetIsa>, JitError> {
    let is_x86_64 = matches!(builder.triple().architecture, Architecture::X86_64);
    let mut settings = beskid_codegen::cranelift_host::production_isa_settings_builder()
        .map_err(|error| JitError::Isa(error.to_string()))?;
    settings
        .set("use_colocated_libcalls", "false")
        .map_err(|error| JitError::Isa(format!("native JIT libcall relocation policy failed: {error}")))?;
    settings
        .set("is_pic", if is_x86_64 { "true" } else { "false" })
        .map_err(|error| JitError::Isa(format!("native JIT PIC policy failed: {error}")))?;
    settings
        .set("opt_level", "speed")
        .map_err(|error| JitError::Isa(format!("native JIT optimization policy failed: {error}")))?;
    builder.finish(settings::Flags::new(settings)).map_err(|error| JitError::Isa(error.to_string()))
}

#[cfg(test)]
mod native_jit_settings_tests {
    use super::*;

    fn policy_isa(triple: &str) -> std::sync::Arc<dyn isa::TargetIsa> {
        let builder = beskid_codegen::target_isa::target_isa_builder(triple).expect("supported target ISA");
        native_jit_isa_from_builder(builder).expect("construct target-derived native JIT ISA")
    }

    #[test]
    fn native_jit_isa_policy_is_target_derived_and_preserves_shared_invariants() {
        let x86_64 = policy_isa("x86_64-unknown-linux-gnu");
        assert!(x86_64.flags().is_pic(), "x86_64 native JIT must materialize symbols through PIC/GOT");
        assert!(!x86_64.flags().use_colocated_libcalls(), "native JIT must use range-independent libcalls");
        assert!(x86_64.flags().preserve_frame_pointers(), "native JIT preserves the shared frame-pointer invariant");
        assert_eq!(x86_64.flags().opt_level(), settings::OptLevel::Speed, "native JIT optimizes generated code");

        let aarch64 = policy_isa("aarch64-unknown-linux-gnu");
        assert!(!aarch64.flags().is_pic(), "non-x86 native JIT must not inherit the x86_64 PIC exception");
        assert!(!aarch64.flags().use_colocated_libcalls(), "native JIT must use range-independent libcalls");
        assert!(aarch64.flags().preserve_frame_pointers(), "native JIT preserves the shared frame-pointer invariant");
        assert_eq!(aarch64.flags().opt_level(), settings::OptLevel::Speed, "native JIT optimizes generated code");
    }

    #[test]
    fn native_jit_isa_has_the_host_target_baseline_and_nothing_above_it() {
        let jit = native_jit_isa().expect("the builder host supports its baseline");
        let target = beskid_codegen::target_isa::host_isa_builder()
            .expect("host baseline")
            .finish(settings::Flags::new(settings::builder()))
            .expect("host ISA");
        let flags = |isa: &dyn TargetIsa| {
            isa.isa_flags().into_iter().map(|value| (value.name, value.as_bool())).collect::<Vec<_>>()
        };
        assert_eq!(flags(jit.as_ref()), flags(target.as_ref()), "JIT and AOT host ISA features differ");
        if jit.triple().architecture == Architecture::X86_64 {
            let enabled = flags(jit.as_ref());
            for setting in beskid_codegen::target_isa::x86_64_baseline_settings() {
                assert!(enabled.contains(&(setting, Some(true))), "{setting} is part of x86-64-v2");
            }
            assert!(enabled.contains(&("has_avx2", Some(false))), "AVX2 is above x86-64-v2");
        }
    }
}
