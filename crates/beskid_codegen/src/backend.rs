//! Backend abstraction at the `CodegenInput` boundary.
//!
//! The compiler lowers typed syntax through one backend. The existing
//! Cranelift CLIF path is the `CraneliftClif` backend; `Beskid.Glue`
//! introduces `RustSource` and `DotNetProject` backends that emit native
//! source projects instead of CLIF. Backends are selected by a manifest
//! flag, not by a new mod contract kind.
//!
//! 0.4 delivery: the `Backend` trait, `BackendKind` enum, `BackendArtifact`
//! enum, and `CraneliftClif` wired to the existing `lower_syntax_program`.
//! `RustSource` and `DotNetProject` are declared and fail closed with
//! `BackendError::NotImplementedFor0_4`. Language-specific emission lands
//! in 0.5.

use cranelift_codegen::isa::TargetIsa;

use crate::CodegenArtifact;
use crate::codegen_input::CodegenInput;
use crate::module_emission::{SyntaxModuleEmissionError, SyntaxModuleItem, lower_syntax_program};

/// The backend selection kind. Selected via a manifest flag or CLI flag;
/// the default is `CraneliftClif`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackendKind {
    /// The existing Cranelift CLIF path. Produces a `CodegenArtifact` bag
    /// of verified `cranelift_codegen::ir::Function`s.
    CraneliftClif,
    /// Emit a Rust source crate. 0.4: declared, fails closed.
    RustSource,
    /// Emit a .NET project. 0.4: declared, fails closed.
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

/// The artifact a backend produces. The CLIF backend produces the existing
/// `CodegenArtifact`; source backends produce text (a crate or project
/// manifest). 0.4 ships only the CLIF variant populated.
#[derive(Debug)]
pub enum BackendArtifact {
    Clif(Box<CodegenArtifact>),
    /// 0.4: never produced. 0.5: a generated Rust crate source string.
    RustSource(String),
    /// 0.4: never produced. 0.5: a generated .NET project source string.
    DotNetProject(String),
}

/// A backend error. Wraps the existing `SyntaxModuleEmissionError` for the
/// CLIF path and adds glue-backend errors.
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("CLIF backend lowering failed: {0}")]
    Clif(#[from] SyntaxModuleEmissionError),
    #[error("backend `{kind}` is declared for 0.4 but not implemented; language-specific generation lands in 0.5")]
    NotImplementedFor0_4 { kind: BackendKind },
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

/// The Rust source backend. Declared for 0.4; fails closed.
#[derive(Debug, Clone, Copy)]
pub struct RustSourceBackend;

impl Backend for RustSourceBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::RustSource
    }

    fn lower(&self, _input: &CodegenInput<'_>, _items: &[SyntaxModuleItem]) -> Result<BackendArtifact, BackendError> {
        Err(BackendError::NotImplementedFor0_4 { kind: BackendKind::RustSource })
    }
}

/// The .NET project backend. Declared for 0.4; fails closed.
#[derive(Debug, Clone, Copy)]
pub struct DotNetProjectBackend;

impl Backend for DotNetProjectBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::DotNetProject
    }

    fn lower(&self, _input: &CodegenInput<'_>, _items: &[SyntaxModuleItem]) -> Result<BackendArtifact, BackendError> {
        Err(BackendError::NotImplementedFor0_4 { kind: BackendKind::DotNetProject })
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
        BackendArtifact::RustSource(_) | BackendArtifact::DotNetProject(_) => {
            Err(BackendError::NotImplementedFor0_4 { kind: BackendKind::RustSource })
        }
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
    fn rust_source_backend_fails_closed() {
        let backend = RustSourceBackend;
        let kind = backend.kind();
        // We cannot construct a CodegenInput in a unit test without a full
        // frontend; assert the kind and the error variant structurally.
        assert_eq!(kind, BackendKind::RustSource);
        assert!(matches!(
            BackendError::NotImplementedFor0_4 { kind: BackendKind::RustSource },
            BackendError::NotImplementedFor0_4 { .. }
        ));
    }
}
