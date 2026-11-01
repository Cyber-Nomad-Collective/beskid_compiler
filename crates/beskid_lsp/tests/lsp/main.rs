//! Integration tests for `beskid_lsp`: one module per topic. The `json_rpc_*` and `process_exit`
//! modules drive the `beskid_lsp` binary over stdio; `semantic_tokens` drives the backend in process.

mod json_rpc_intellisense;
mod json_rpc_multi_project;
mod json_rpc_shared_corelib;
mod process_exit;
mod semantic_tokens;
