//! The one owner of the Cranelift target ISA baseline (OpenSpec `compiler--build-pipeline--backends-jit-aot`,
//! "Target CPU baseline" and "No Cranelift runtime helpers").
//!
//! AOT object emission, the JIT and the compiler tests build every ISA through [`target_isa_builder`] or
//! [`host_isa_builder`], so one target always gets the same instruction set.
//!
//! - x86_64 targets use the x86-64-v2 level of the x86-64 psABI (SSE3, SSSE3, SSE4.1, SSE4.2, POPCNT,
//!   CMPXCHG16B, LAHF-SAHF), the Cranelift preset `x86-64-v2`. Without SSSE3 Cranelift lowers a CLIF
//!   `swizzle` or two-input `shuffle` to the `__cranelift_x86_pshufb` helper (`lower_pshufb` in
//!   `cranelift-codegen` `src/isa/x64/lower.isle`), which no runtime kit defines.
//! - aarch64 targets use the Cranelift default, Armv8.0-A with no optional extension.
//!
//! Host CPU detection never adds a feature: a JIT run and an AOT build of one program use one baseline.

use cranelift_codegen::ir::{ExternalName, LibCall};
use cranelift_codegen::isa;
use cranelift_codegen::settings::Configurable;
use target_lexicon::{Architecture, Triple};

/// Cranelift preset and psABI level of every x86_64 target.
pub const X86_64_BASELINE: &str = "x86-64-v2";

/// The x86-64-v2 features as `(Cranelift setting, CPUID name)`, for the host check and its message.
const X86_64_BASELINE_FEATURES: [(&str, &str); 6] = [
    ("has_sse3", "sse3"),
    ("has_ssse3", "ssse3"),
    ("has_sse41", "sse4.1"),
    ("has_sse42", "sse4.2"),
    ("has_popcnt", "popcnt"),
    ("has_cmpxchg16b", "cmpxchg16b"),
];

/// Failure to build a baseline ISA.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetIsaError {
    /// Cranelift has no backend for the triple.
    Lookup { triple: String, message: String },
    /// Cranelift rejected the baseline preset.
    Baseline { triple: String, message: String },
    /// The host CPU lacks a feature of the host target baseline.
    HostBelowBaseline { triple: String, baseline: &'static str, missing: Vec<&'static str> },
}

impl std::fmt::Display for TargetIsaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Lookup { triple, message } => write!(f, "no Cranelift backend for target `{triple}`: {message}"),
            Self::Baseline { triple, message } => {
                write!(f, "Cranelift rejected the CPU baseline of target `{triple}`: {message}")
            }
            Self::HostBelowBaseline { triple, baseline, missing } => write!(
                f,
                "the host CPU does not support the {baseline} baseline of target `{triple}`; missing: {}",
                missing.join(", ")
            ),
        }
    }
}

impl std::error::Error for TargetIsaError {}

/// Name of the baseline of `triple`, for diagnostics.
pub fn baseline_name(triple: &Triple) -> &'static str {
    match triple.architecture {
        Architecture::X86_64 => X86_64_BASELINE,
        Architecture::Aarch64(_) => "armv8.0-a",
        _ => "cranelift-default",
    }
}

/// ISA builder for `triple` with its baseline and no other CPU feature.
pub fn target_isa_builder(triple: &str) -> Result<isa::Builder, TargetIsaError> {
    let mut builder = isa::lookup_by_name(triple)
        .map_err(|error| TargetIsaError::Lookup { triple: triple.to_owned(), message: error.to_string() })?;
    apply_baseline(&mut builder)?;
    Ok(builder)
}

/// ISA builder for the host target with its baseline, after a check that the host CPU supports it.
pub fn host_isa_builder() -> Result<isa::Builder, TargetIsaError> {
    let triple = Triple::host();
    let mut builder = isa::lookup(triple.clone())
        .map_err(|error| TargetIsaError::Lookup { triple: triple.to_string(), message: error.to_string() })?;
    apply_baseline(&mut builder)?;
    let missing = missing_host_baseline_features(&triple);
    if !missing.is_empty() {
        return Err(TargetIsaError::HostBelowBaseline {
            triple: triple.to_string(),
            baseline: baseline_name(&triple),
            missing,
        });
    }
    Ok(builder)
}

fn apply_baseline(builder: &mut isa::Builder) -> Result<(), TargetIsaError> {
    if matches!(builder.triple().architecture, Architecture::X86_64) {
        let triple = builder.triple().to_string();
        builder
            .enable(X86_64_BASELINE)
            .map_err(|error| TargetIsaError::Baseline { triple, message: error.to_string() })?;
    }
    Ok(())
}

/// Baseline features that the host CPU does not report (CPUID names).
fn missing_host_baseline_features(triple: &Triple) -> Vec<&'static str> {
    if !matches!(triple.architecture, Architecture::X86_64) {
        return Vec::new();
    }
    X86_64_BASELINE_FEATURES
        .iter()
        .filter(|(setting, _)| !host_has_x86_feature(setting))
        .map(|(_, name)| *name)
        .collect()
}

#[cfg(target_arch = "x86_64")]
fn host_has_x86_feature(setting: &str) -> bool {
    match setting {
        "has_sse3" => std::arch::is_x86_feature_detected!("sse3"),
        "has_ssse3" => std::arch::is_x86_feature_detected!("ssse3"),
        "has_sse41" => std::arch::is_x86_feature_detected!("sse4.1"),
        "has_sse42" => std::arch::is_x86_feature_detected!("sse4.2"),
        "has_popcnt" => std::arch::is_x86_feature_detected!("popcnt"),
        "has_cmpxchg16b" => std::arch::is_x86_feature_detected!("cmpxchg16b"),
        _ => false,
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn host_has_x86_feature(_setting: &str) -> bool {
    false
}

/// The x86-64-v2 Cranelift settings, for tests that check a built ISA.
pub fn x86_64_baseline_settings() -> impl Iterator<Item = &'static str> {
    X86_64_BASELINE_FEATURES.iter().map(|(setting, _)| *setting)
}

/// Symbol of a Cranelift runtime helper libcall. No runtime kit defines one, so generated code must not call
/// it. The names are those of `cranelift_module::default_libcall_names`; the other libcalls are C library
/// functions (`memcpy`, `floorf`, `fma`, `__tls_get_addr`, ...) that the platform links.
pub fn cranelift_runtime_helper(libcall: LibCall) -> Option<&'static str> {
    match libcall {
        LibCall::Probestack => Some("__cranelift_probestack"),
        LibCall::X86Pshufb => Some("__cranelift_x86_pshufb"),
        _ => None,
    }
}

/// The E4004 message for compiled code that calls a Cranelift runtime helper; AOT and the JIT both use it.
pub fn runtime_helper_message(function: &str, symbol: &str, target: &str) -> String {
    format!(
        "[E4004] function `{function}` calls the Cranelift runtime helper `{symbol}` on target `{target}`; no \
         runtime kit defines it (the target CPU baseline must cover the instruction)"
    )
}

/// First Cranelift runtime helper that a relocation of compiled code names.
pub fn first_runtime_helper_call<'a>(targets: impl IntoIterator<Item = &'a ExternalName>) -> Option<&'static str> {
    targets.into_iter().find_map(|name| match name {
        ExternalName::LibCall(libcall) => cranelift_runtime_helper(*libcall),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cranelift_codegen::settings;

    fn finish(builder: isa::Builder) -> std::sync::Arc<dyn isa::TargetIsa> {
        builder.finish(settings::Flags::new(settings::builder())).expect("ISA")
    }

    fn isa_flag(isa: &dyn isa::TargetIsa, name: &str) -> Option<bool> {
        isa.isa_flags().into_iter().find(|value| value.name == name).and_then(|value| value.as_bool())
    }

    #[test]
    fn every_x86_64_target_enables_exactly_x86_64_v2() {
        for triple in ["x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc"] {
            let isa = finish(target_isa_builder(triple).expect("x86_64 builder"));
            for setting in x86_64_baseline_settings() {
                assert_eq!(isa_flag(isa.as_ref(), setting), Some(true), "{triple}: {setting}");
            }
            for above in ["has_avx", "has_avx2", "has_bmi1", "has_bmi2", "has_fma", "has_lzcnt", "has_avx512f"] {
                assert_eq!(isa_flag(isa.as_ref(), above), Some(false), "{triple}: {above} is above x86-64-v2");
            }
        }
    }

    #[test]
    fn aarch64_targets_keep_the_armv8_0_default() {
        let isa = finish(target_isa_builder("aarch64-apple-darwin").expect("aarch64 builder"));
        for extension in ["has_lse", "has_pauth", "has_fp16"] {
            assert_eq!(isa_flag(isa.as_ref(), extension), Some(false), "{extension}");
        }
    }

    #[test]
    fn the_host_builder_uses_the_host_target_baseline() {
        let host = finish(host_isa_builder().expect("the builder host supports its baseline"));
        let named = finish(target_isa_builder(&Triple::host().to_string()).expect("host triple builder"));
        assert_eq!(host.isa_flags().len(), named.isa_flags().len());
        for (left, right) in host.isa_flags().into_iter().zip(named.isa_flags()) {
            assert_eq!((left.name, left.as_bool()), (right.name, right.as_bool()));
        }
    }

    #[test]
    fn only_cranelift_helpers_are_runtime_helper_libcalls() {
        assert_eq!(cranelift_runtime_helper(LibCall::X86Pshufb), Some("__cranelift_x86_pshufb"));
        assert_eq!(cranelift_runtime_helper(LibCall::Probestack), Some("__cranelift_probestack"));
        for libc in [LibCall::Memcpy, LibCall::FloorF32, LibCall::FmaF64, LibCall::ElfTlsGetAddr] {
            assert_eq!(cranelift_runtime_helper(libc), None, "{libc}");
        }
    }

    #[test]
    fn host_below_baseline_names_the_baseline_and_the_features() {
        let error = TargetIsaError::HostBelowBaseline {
            triple: "x86_64-unknown-linux-gnu".into(),
            baseline: X86_64_BASELINE,
            missing: vec!["sse4.2", "popcnt"],
        };
        let text = error.to_string();
        assert!(text.contains("x86-64-v2") && text.contains("sse4.2") && text.contains("popcnt"), "{text}");
    }

    #[test]
    fn runtime_helper_message_names_code_function_symbol_and_target() {
        let text = runtime_helper_message("Aes.Encrypt", "__cranelift_x86_pshufb", "x86_64-unknown-linux-gnu");
        assert!(text.starts_with("[E4004] "), "{text}");
        for part in ["Aes.Encrypt", "__cranelift_x86_pshufb", "x86_64-unknown-linux-gnu"] {
            assert!(text.contains(part), "{text}");
        }
    }
}
