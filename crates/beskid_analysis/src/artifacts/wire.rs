//! Postcard encode/decode for `SourceUnit` and `UnitHir` snapshots.

use beskid_artifacts::{
    AstUnitSnapshot, HirUnitSnapshot, UnitArtifactMeta, content_fingerprint,
    grammar_revision,
};
use postcard::{Error as PostcardError};

use crate::artifacts::hir_wire::{decode_hir_program as decode_hir_marker, encode_hir_program as encode_hir_marker};
use crate::projects::assembly::{SourceUnit, UnitHir};
use crate::syntax::{Program, Spanned};

pub fn encode_syntax_program(program: &Spanned<Program>) -> Result<Vec<u8>, PostcardError> {
    postcard::to_allocvec(program)
}

pub fn decode_syntax_program(bytes: &[u8]) -> Result<Spanned<Program>, PostcardError> {
    postcard::from_bytes(bytes)
}

pub fn source_unit_snapshot(unit: &SourceUnit, imports: &[String]) -> Result<AstUnitSnapshot, PostcardError> {
    let fp = content_fingerprint(&unit.source);
    let program_wire = encode_syntax_program(&unit.program)?;
    Ok(AstUnitSnapshot::new(
        UnitArtifactMeta {
            content_fingerprint: fp,
            schema_version: beskid_artifacts::ARTIFACT_SCHEMA_VERSION,
            grammar_rev: grammar_revision().to_string(),
            logical_name: unit.logical_name.clone(),
            source_path: unit.path.clone(),
            source_len: unit.source.len(),
            imports: imports.to_vec(),
        },
        program_wire,
    ))
}

pub fn hir_unit_snapshot(
    content_fingerprint: &str,
    hir_unit: &UnitHir,
) -> Result<HirUnitSnapshot, PostcardError> {
    let hir_wire = encode_hir_marker(&hir_unit.hir, content_fingerprint)?;
    Ok(HirUnitSnapshot::new(
        content_fingerprint.to_string(),
        hir_wire,
    ))
}

pub fn source_unit_from_ast_snapshot(
    snapshot: &AstUnitSnapshot,
    source: &str,
) -> Result<SourceUnit, PostcardError> {
    let program = decode_syntax_program(&snapshot.program_wire)?;
    Ok(SourceUnit {
        logical_name: snapshot.meta.logical_name.clone(),
        path: crate::paths::unit_path_key(&snapshot.meta.source_path),
        source: source.to_string(),
        program,
    })
}

pub fn unit_hir_from_hir_snapshot(
    path: std::path::PathBuf,
    source_unit: &SourceUnit,
    snapshot: &HirUnitSnapshot,
) -> Result<UnitHir, String> {
    let hir = decode_hir_marker(
        &snapshot.hir_wire,
        source_unit,
        &snapshot.content_fingerprint,
    )?;
    Ok(UnitHir {
        path: crate::paths::unit_path_key(&path),
        hir,
    })
}
