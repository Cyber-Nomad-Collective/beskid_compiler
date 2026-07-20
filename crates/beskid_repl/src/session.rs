use beskid_engine::{Engine, JitError};

use crate::eval::{self, EvalOutcome};

/// Persistent JIT session for snippet evaluation.
pub struct ReplSession {
    engine: Engine,
}

impl ReplSession {
    pub fn new() -> Self {
        Self::try_new().expect("failed to initialize exact ABI-v5 REPL runtime kit")
    }

    /// Fallible form of [`Self::new`]; missing or tampered exact kits fail closed.
    pub fn try_new() -> Result<Self, JitError> {
        Ok(Self {
            engine: Engine::try_new()?,
        })
    }

    /// Drop the current JIT module and reload the same validated exact ABI-v5 runtime kit.
    pub fn reset(&mut self) {
        self.engine
            .reload_runtime_kit()
            .expect("failed to reload exact ABI-v5 REPL runtime kit");
    }

    /// Construct a session around an already-validated exact-kit [`Engine`] (tests).
    pub fn from_engine(engine: Engine) -> Self {
        Self { engine }
    }

    /// Evaluate a snippet (not a colon-command).
    pub fn eval(&mut self, snippet: &str) -> EvalOutcome {
        eval::eval_snippet(&mut self.engine, snippet)
    }

    /// Print the inferred return type of an expression snippet, or `unit` for statements.
    pub fn type_of(&mut self, snippet: &str) -> EvalOutcome {
        eval::type_of_snippet(snippet)
    }
}

impl Default for ReplSession {
    fn default() -> Self {
        Self::new()
    }
}
