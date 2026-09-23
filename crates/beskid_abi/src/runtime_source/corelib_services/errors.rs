/// Why a native Corelib import was denied before it could enter a backend artifact.
///
/// This is deliberately separate from user-FFI validation. A Corelib service is not a user
/// extern: its authority is the exact `(source path, service name, adapter)` declaration plus
/// the selected canonical ABI-v5 target contract. Glue does not participate in this decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorelibServiceImportPreflightError {
    InvalidManifest,
    NonCanonicalManifest { target: String },
    UnauthorizedDeclaration { name: String, symbol: String, source_path: String },
    MissingManifestService { name: String },
    DuplicateManifestDeclaration { name: String },
    TargetCoverageMismatch { name: String, expected: Vec<String>, actual: Vec<String> },
    DuplicateTargetBinding { name: String, target: String },
    AdapterMismatch { name: String, expected: String, actual: String },
    ImplementationMismatch { name: String, expected: String, actual: String, target: String },
    TargetShapeMismatch { name: String, target: String },
}

impl std::fmt::Display for CorelibServiceImportPreflightError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidManifest => f.write_str("Corelib import requires a valid ABI-v5 manifest"),
            Self::NonCanonicalManifest { target } => {
                write!(f, "Corelib import requires the canonical ABI-v5 manifest for `{target}`")
            }
            Self::UnauthorizedDeclaration { name, symbol, source_path } => {
                write!(f, "unauthorized Corelib import `{name}` / `{symbol}` from `{source_path}`")
            }
            Self::MissingManifestService { name } => write!(f, "Corelib import `{name}` has no manifest service"),
            Self::DuplicateManifestDeclaration { name } => {
                write!(f, "Corelib import `{name}` has duplicate manifest declarations")
            }
            Self::TargetCoverageMismatch { name, expected, actual } => {
                write!(f, "Corelib import `{name}` target coverage mismatch: expected={expected:?}, actual={actual:?}")
            }
            Self::DuplicateTargetBinding { name, target } => {
                write!(f, "Corelib import `{name}` has duplicate `{target}` target bindings")
            }
            Self::AdapterMismatch { name, expected, actual } => {
                write!(f, "Corelib import `{name}` adapter mismatch: expected `{expected}`, actual `{actual}`")
            }
            Self::ImplementationMismatch { name, expected, actual, target } => write!(
                f,
                "Corelib import `{name}` implementation mismatch for `{target}`: expected `{expected}`, actual `{actual}`"
            ),
            Self::TargetShapeMismatch { name, target } => {
                write!(f, "Corelib import `{name}` has a target-shape mismatch at `{target}`")
            }
        }
    }
}

impl std::error::Error for CorelibServiceImportPreflightError {}
