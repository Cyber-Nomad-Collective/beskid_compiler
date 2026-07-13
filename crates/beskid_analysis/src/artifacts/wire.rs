//! Postcard encode/decode for expanded syntax unit snapshots.

use beskid_artifacts::{AstUnitSnapshot, UnitArtifactMeta, content_fingerprint, grammar_revision};
use postcard::Error as PostcardError;

use crate::projects::assembly::SourceUnit;
use crate::syntax::{Program, Spanned};

pub fn encode_syntax_program(program: &Spanned<Program>) -> Result<Vec<u8>, PostcardError> {
    postcard::to_allocvec(program)
}

pub fn decode_syntax_program(bytes: &[u8]) -> Result<Spanned<Program>, PostcardError> {
    postcard::from_bytes(bytes)
}

pub fn source_unit_snapshot(
    unit: &SourceUnit,
    imports: &[String],
) -> Result<AstUnitSnapshot, PostcardError> {
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
