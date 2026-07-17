//! Unit artifact snapshot encoding for on-disk cache (postcard wire format).

mod wire;

pub use wire::{
    decode_syntax_program, encode_syntax_program, source_unit_from_ast_snapshot,
    source_unit_snapshot,
};
