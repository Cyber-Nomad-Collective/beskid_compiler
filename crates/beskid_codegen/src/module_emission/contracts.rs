use beskid_isle::FunctionEmissionError;
use cranelift_module::ModuleError;

use crate::CodegenInput;

#[derive(Debug, thiserror::Error)]
pub enum SyntaxModuleEmissionError {
    #[error("module declaration failed: {0}")]
    Module(#[from] ModuleError),
    /// Pre-formatted with [`FunctionEmissionError::display_with_db`] so FAIL lines include
    /// construct and source range, not only `#gN:nN`.
    #[error("syntax ISLE emission failed: {0}")]
    Emission(String),
    #[error("syntax module declares duplicate symbol `{0}`")]
    DuplicateSymbol(String),
}

pub(super) fn emission_error(
    input: &CodegenInput<'_>,
    error: FunctionEmissionError,
) -> SyntaxModuleEmissionError {
    SyntaxModuleEmissionError::Emission(error.display_with_db(input.database()))
}

pub(super) fn emission_verification(message: impl Into<String>) -> SyntaxModuleEmissionError {
    SyntaxModuleEmissionError::Emission(format!("Verification({})", message.into()))
}

