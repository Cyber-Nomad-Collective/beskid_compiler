//! Backend abstraction at the `CodegenInput` boundary.
//!
//! The compiler lowers typed syntax through one backend. The Cranelift CLIF
//! path is the `CraneliftClif` backend. `RustSource` emits the generated Rust
//! Glue packet that `beskid build --backend glue-rust` links. `DotNetProject`
//! is a reserved selection that is unavailable in 0.6 and always fails closed.
//! Backends are selected by the `beskid build --backend` flag, not by a mod
//! contract kind.
//!

use cranelift_codegen::isa::TargetIsa;

use crate::CodegenArtifact;
use crate::codegen_input::CodegenInput;
use crate::module_emission::{SyntaxModuleEmissionError, SyntaxModuleItem, lower_syntax_program};

/// The backend selection kind, selected by `beskid build --backend`; the
/// default is `CraneliftClif`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackendKind {
    /// The existing Cranelift CLIF path. Produces a `CodegenArtifact` bag
    /// of verified `cranelift_codegen::ir::Function`s.
    CraneliftClif,
    /// Rust Glue (`glue-rust`): the generated Rust Glue packet for a shared
    /// consumer and its Rust owners.
    RustSource,
    /// .NET Glue (`glue-dotnet`): unavailable in 0.6; every use fails closed.
    DotNetProject,
}

impl std::fmt::Display for BackendKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl BackendKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CraneliftClif => "clif",
            Self::RustSource => "glue-rust",
            Self::DotNetProject => "glue-dotnet",
        }
    }

    pub fn parse(value: &str) -> Result<Self, BackendKindParseError> {
        match value {
            "clif" => Ok(Self::CraneliftClif),
            "glue-rust" => Ok(Self::RustSource),
            "glue-dotnet" => Ok(Self::DotNetProject),
            _ => Err(BackendKindParseError(value.to_owned())),
        }
    }
}

impl std::str::FromStr for BackendKind {
    type Err = BackendKindParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendKindParseError(pub String);

impl std::fmt::Display for BackendKindParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown backend `{}`, expected clif|glue-rust|glue-dotnet", self.0)
    }
}

impl std::error::Error for BackendKindParseError {}

/// The artifact a backend produces: the CLIF `CodegenArtifact` or the
/// generated Rust Glue packet.
#[derive(Debug)]
pub enum BackendArtifact {
    Clif(Box<CodegenArtifact>),
    RustSource(crate::glue::GlueArtifact),
}

/// A backend error. Wraps the existing `SyntaxModuleEmissionError` for the
/// CLIF path and adds glue-backend errors.
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("CLIF backend lowering failed: {0}")]
    Clif(#[from] SyntaxModuleEmissionError),
    #[error("backend `{kind}` is unavailable in 0.6")]
    Unavailable { kind: BackendKind },
    #[error("unsupported Rust Glue logical type: {0}")]
    UnsupportedRustGlueType(String),
    #[error("invalid Rust Glue native library: {0}")]
    InvalidRustGlueLibrary(String),
    #[error("stale Rust Glue item {key:?} ({symbol})")]
    StaleRustGlueItem { key: beskid_queries::AstNodeKey, symbol: String },
    #[error("Rust Glue fact unavailable for {key:?}: {message}")]
    RustGlueFact { key: beskid_queries::AstNodeKey, message: String },
    #[error("missing declared Rust Glue binding for {0:?}")]
    MissingRustGlueBinding(beskid_queries::AstNodeKey),
    #[error("invalid Rust Glue symbol: {0}")]
    InvalidRustGlueSymbol(String),
    #[error("duplicate Rust Glue symbol: {0}")]
    DuplicateRustGlueSymbol(String),
    #[error(transparent)]
    GlueArtifact(#[from] crate::glue::GlueArtifactError),
    #[error("expected CLIF artifact, received {kind}")]
    WrongArtifactKind { kind: BackendKind },
}

/// A codegen backend. Implementations lower typed syntax through one
/// concrete backend. The trait is object-safe so the CLI can dispatch
/// through `dyn Backend`.
pub trait Backend {
    fn kind(&self) -> BackendKind;
    fn lower(&self, input: &CodegenInput<'_>, items: &[SyntaxModuleItem]) -> Result<BackendArtifact, BackendError>;
}

/// The existing Cranelift CLIF backend. Wraps `lower_syntax_program`.
pub struct CraneliftClifBackend<'a> {
    pub isa: &'a dyn TargetIsa,
}

impl<'a> Backend for CraneliftClifBackend<'a> {
    fn kind(&self) -> BackendKind {
        BackendKind::CraneliftClif
    }

    fn lower(&self, input: &CodegenInput<'_>, items: &[SyntaxModuleItem]) -> Result<BackendArtifact, BackendError> {
        let artifact = lower_syntax_program(input, self.isa, items)?;
        Ok(BackendArtifact::Clif(Box::new(artifact)))
    }
}

/// Emits a closed Rust project for an explicitly named native library.
#[derive(Debug, Clone)]
pub struct RustSourceBackend {
    pub native_library: String,
}

impl Backend for RustSourceBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::RustSource
    }

    fn lower(&self, input: &CodegenInput<'_>, items: &[SyntaxModuleItem]) -> Result<BackendArtifact, BackendError> {
        Ok(BackendArtifact::RustSource(crate::glue::emit(input, items, &self.native_library)?))
    }
}

/// The .NET Glue backend. Unavailable in 0.6; it always fails closed.
#[derive(Debug, Clone, Copy)]
pub struct DotNetProjectBackend;

impl Backend for DotNetProjectBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::DotNetProject
    }

    fn lower(&self, _input: &CodegenInput<'_>, _items: &[SyntaxModuleItem]) -> Result<BackendArtifact, BackendError> {
        Err(BackendError::Unavailable { kind: BackendKind::DotNetProject })
    }
}

/// Lower typed syntax through the selected backend.
pub fn lower_with_backend(
    backend: &dyn Backend,
    input: &CodegenInput<'_>,
    items: &[SyntaxModuleItem],
) -> Result<BackendArtifact, BackendError> {
    backend.lower(input, items)
}

/// Extract the CLIF artifact from a backend result, erroring when the
/// selected backend did not produce CLIF. The existing AOT/JIT consumers
/// take `CodegenArtifact` directly; this helper bridges the new
/// `BackendArtifact` enum to them.
pub fn expect_clif(artifact: BackendArtifact) -> Result<CodegenArtifact, BackendError> {
    match artifact {
        BackendArtifact::Clif(artifact) => Ok(*artifact),
        BackendArtifact::RustSource(_) => Err(BackendError::WrongArtifactKind { kind: BackendKind::RustSource }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_kind_round_trips() {
        for kind in [BackendKind::CraneliftClif, BackendKind::RustSource, BackendKind::DotNetProject] {
            assert_eq!(BackendKind::parse(kind.as_str()).unwrap(), kind);
        }
    }

    #[test]
    fn unknown_backend_kind_is_rejected() {
        assert!(BackendKind::parse("wat").is_err());
    }

    #[test]
    fn dotnet_backend_is_unavailable() {
        assert_eq!(BackendError::Unavailable { kind: BackendKind::DotNetProject }.to_string(), "backend `glue-dotnet` is unavailable in 0.6");
    }

    #[test]
    fn rust_backend_requires_explicit_native_library() {
        let backend = RustSourceBackend { native_library: "manual".into() };
        assert_eq!(backend.kind(), BackendKind::RustSource);
    }
}
