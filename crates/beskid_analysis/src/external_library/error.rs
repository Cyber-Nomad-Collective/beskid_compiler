//! `LibraryResolveError` — surfaced by `ExternalLibrary` providers and the registry.

use thiserror::Error;

/// Failure resolving a logical library name through an `ExternalLibrary` provider or the closed
/// registry.
///
/// The CLI maps these into structured diagnostics including the provider id, host key, and
/// logical name (per spec).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum LibraryResolveError {
    /// Provider id is not registered in the closed registry.
    #[error("unknown ExternalLibrary provider `{provider}` (known providers: {known})")]
    UnknownProvider { provider: String, known: String },

    /// Provider does not support the requested host key.
    #[error("provider `{provider}` does not support host `{host}` (provider host_key = `{provider_host}`)")]
    HostUnsupported { provider: String, provider_host: String, host: String },

    /// Provider has no mapping for the requested logical library name.
    #[error("provider `{provider}` (host = `{host}`) cannot resolve logical library `{logical}`: {detail}")]
    UnknownLogicalName { provider: String, host: String, logical: String, detail: String },

    /// Logical library name is empty or otherwise invalid.
    #[error("logical library name `{logical}` is invalid: {detail}")]
    InvalidLogicalName { logical: String, detail: String },
}
