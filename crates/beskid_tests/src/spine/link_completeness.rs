//! Link-plan lowering must pass `validate_artifact` for project entrypoints.

use beskid_codegen::validate_artifact;

#[cfg(feature = "slow")]
use crate::projects::fixture_harness::lower_corelib_tests_entrypoint;
#[cfg(feature = "slow")]
use beskid_codegen::CodegenArtifact;

/// The ANSI cursor builder must travel through the production syntax-only link surface.
///
/// This deliberately replaces the retired intermediate link-plan probe. The generated ISLE artifact is
/// the executable link-plan authority: an obsolete link plan says nothing about whether a
/// `CodegenInput` can select every syntax lowering rule required by the canonical corelib.
#[cfg(feature = "slow")]
#[test]
fn ansi_cursor_builder_home_syntax_isle_link_plan_validates() {
    let artifact = lower_corelib_tests_entrypoint("console/AnsiBuildersTests.bd", "ansi_cursor_builder_home");
    validate_artifact(&artifact).expect("ansi_cursor_builder_home syntax ISLE link plan must validate");

    for expected_symbol in ["Home#syntax_", "IntoSequence#syntax_", "WhenEnabled#syntax_", "Esc#syntax_"] {
        assert!(
            artifact.functions.iter().any(|function| function.name.contains(expected_symbol)),
            "syntax link closure must retain {expected_symbol}; emitted {:?}",
            artifact.functions.iter().map(|function| function.name.as_str()).collect::<Vec<_>>()
        );
    }
}

/// Byte-level regression for the normative `ESC [` CSI framing in `Ansi.Escape`.
///
/// The sequence is assembled at runtime, so it has no one `ESC[1;31m` static global. The golden
/// locks the exact ISLE-owned fragments instead: `Esc()` owns the one-byte ESC control character,
/// while the entry test owns the CSI body, final byte, and expected-message suffix.
#[cfg(feature = "slow")]
#[test]
fn ansi_csi_bold_red_syntax_isle_preserves_csi_byte_golden() {
    let artifact = lower_corelib_tests_entrypoint("console/AnsiEscapeTests.bd", "ansi_csi_bold_red");
    validate_artifact(&artifact).expect("ansi_csi_bold_red syntax ISLE link plan must validate");

    assert_literal_byte_goldens(&artifact, &[b"\x1b", b"1;31", b"m", b"[1;31m"]);
}

#[cfg(feature = "slow")]
fn assert_literal_byte_goldens(artifact: &CodegenArtifact, expected_literals: &[&[u8]]) {
    let emitted = artifact.string_literals.values().map(Vec::as_slice).collect::<Vec<_>>();
    for expected in expected_literals {
        assert!(emitted.contains(expected), "missing byte golden {expected:?}; emitted {emitted:?}");
    }
}
