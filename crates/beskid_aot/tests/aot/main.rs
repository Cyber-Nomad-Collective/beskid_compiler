//! Integration tests for `beskid_aot`: one module per topic. `tests/fixtures/` holds prebuilt
//! inputs that `src/object_symbols.rs` also reads, so it stays beside this directory.

mod abi_v5_runtime_kit;
mod glue_admission;
mod glue_declarations;
mod library_emission;
mod library_pair;
#[cfg(unix)]
mod link_tool_provenance;
mod mod_artifact;
mod prepared_syntax;
