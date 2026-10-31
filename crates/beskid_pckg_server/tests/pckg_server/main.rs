//! Integration tests for `beskid_pckg_server`: one module per HTTP surface or contract. Every
//! test builds its own router and stores in process; `support` holds the shared request and
//! artifact builders.

mod api_key_auth_http;
mod artifact_http;
mod community_http;
mod embed_http;
mod http_contract;
mod operations_http;
mod package_contract;
mod support;
