//! HIR artifact marker: full HIR is rebuilt from the AST snapshot on cold load.

use serde::{Deserialize, Serialize};

use crate::hir::HirProgram;
use crate::projects::assembly::{build_hir_units, SourceUnit};
use crate::syntax::Spanned;

/// Marker written to `hir.bin`; lowering is deterministic from `ast.bin`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HirCacheMarker {
    pub content_fingerprint: String,
    pub program_span_start: usize,
    pub program_span_end: usize,
}

pub fn encode_hir_program(
    hir: &Spanned<HirProgram>,
    content_fingerprint: &str,
) -> Result<Vec<u8>, postcard::Error> {
    let marker = HirCacheMarker {
        content_fingerprint: content_fingerprint.to_string(),
        program_span_start: hir.span.start,
        program_span_end: hir.span.end,
    };
    postcard::to_allocvec(&marker)
}

pub fn decode_hir_program(
    bytes: &[u8],
    unit: &SourceUnit,
    expected_fingerprint: &str,
) -> Result<Spanned<HirProgram>, String> {
    let marker: HirCacheMarker =
        postcard::from_bytes(bytes).map_err(|err| format!("hir marker decode: {err}"))?;
    if marker.content_fingerprint != expected_fingerprint {
        return Err("hir marker fingerprint mismatch".to_string());
    }
    build_hir_units(std::slice::from_ref(unit))
        .into_iter()
        .next()
        .map(|unit_hir| unit_hir.hir)
        .ok_or_else(|| "hir rebuild failed".to_string())
}
