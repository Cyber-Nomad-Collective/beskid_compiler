//! Canonical SDK callback authority comes from compiled source bytes, not package names.
use beskid_codegen::native_mod::sdk_sources::{canonical_sdk_sources, verify_canonical_sdk_source};

#[test]
fn native_sdk_source_authority_rejects_source_replacement_and_unknown_paths() {
    let sources = canonical_sdk_sources();
    let requests = sources.iter().find(|source| source.path() == "src/Beskid/Compiler/NativeRequests.bd").unwrap();
    assert!(verify_canonical_sdk_source(requests.path(), requests.bytes()).is_ok());
    let mut forged = requests.bytes().to_vec();
    forged.extend_from_slice(b"\n// caller replacement\n");
    assert!(verify_canonical_sdk_source(requests.path(), &forged).is_err());
    assert!(verify_canonical_sdk_source("src/Fake/NativeRequests.bd", requests.bytes()).is_err());
    assert!(sources.iter().any(|source| source.path() == "src/Beskid/Syntax/syntax.schema.json"));
    assert!(sources.iter().any(|source| source.path() == "corelib_compiler_sdk.bproj"));
    assert!(sources.windows(2).all(|pair| pair[0].path() < pair[1].path()));
}
