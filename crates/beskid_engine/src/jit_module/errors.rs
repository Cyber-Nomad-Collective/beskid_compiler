use std::fmt;

use beskid_codegen::cranelift_host::{ExternDeclarationError, HostError};
use cranelift_module::ModuleError;

/// Failure to build the ISA, declare/define Cranelift module objects, or resolve a symbol name.
#[derive(Debug)]
pub enum JitError {
    Isa(String),
    Module(Box<ModuleError>),
    MissingFunction(String),
    RuntimeKit(String),
}

impl fmt::Display for JitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JitError::Isa(msg) => write!(f, "{msg}"),
            JitError::Module(err) => write!(f, "{err}"),
            JitError::MissingFunction(name) => write!(f, "missing function `{name}`"),
            JitError::RuntimeKit(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for JitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            JitError::Module(err) => Some(err.as_ref()),
            _ => None,
        }
    }
}

impl From<ModuleError> for JitError {
    fn from(error: ModuleError) -> Self {
        Self::Module(Box::new(error))
    }
}

impl From<HostError> for JitError {
    fn from(value: HostError) -> Self {
        match value {
            HostError::MissingSymbol(name) => JitError::MissingFunction(name),
            HostError::InvalidGlobalValue => JitError::Isa("invalid global value for extern data symbol".to_owned()),
        }
    }
}

impl From<ExternDeclarationError> for JitError {
    fn from(value: ExternDeclarationError) -> Self {
        match value {
            ExternDeclarationError::InvalidSignature(message) => JitError::Isa(message),
            ExternDeclarationError::Module(error) => JitError::Module(Box::new(error)),
        }
    }
}
