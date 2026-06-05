//! Unit artifact snapshot encoding for on-disk cache (postcard wire format).

mod hir_wire;
mod wire;

pub use hir_wire::{decode_hir_program, encode_hir_program};
pub use wire::{
    decode_syntax_program, encode_syntax_program, hir_unit_snapshot, source_unit_from_ast_snapshot,
    source_unit_snapshot, unit_hir_from_hir_snapshot,
};
