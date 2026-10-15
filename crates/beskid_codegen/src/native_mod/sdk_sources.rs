//! Read-only canonical SDK source correspondence. Issued callbacks additionally require
//! current registered package, generation and canonical layout authority.
pub use beskid_abi::sdk_source::{
    CanonicalSdkSource, SdkSourceError, canonical_sdk_source_digest, canonical_sdk_sources, verify_canonical_sdk_source,
};
