//! Integration tests for `beskid_isle`: one module per lowering construct.

mod array_memory;
mod block_range_for;
mod block_sequence;
mod bulk_call;
mod clif_block;
mod clif_opcode_lowering;
mod clif_primitives;
mod compile;
mod direct_calls;
mod enum_match;
mod function_emitter;
mod if_else;
mod iterator_for;
mod leaf_clif;
mod locals;
mod managed_local_roots;
mod rule_coverage;
mod runtime_memory_set;
mod statement_emitter;
mod string_literals;
mod struct_memory;
mod while_transfer;
