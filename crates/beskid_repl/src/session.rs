use beskid_aot::RuntimeLinkProfile;
use beskid_engine::Engine;

use crate::eval::{self, EvalOutcome};

/// Persistent JIT session for snippet evaluation.
pub struct ReplSession {
    engine: Engine,
    runtime_link_profile: RuntimeLinkProfile,
}

impl ReplSession {
    pub fn new() -> Self {
        Self::with_link_profile(RuntimeLinkProfile::Std)
    }

    pub fn with_link_profile(runtime_link_profile: RuntimeLinkProfile) -> Self {
        Self {
            engine: Engine::with_link_profile(runtime_link_profile),
            runtime_link_profile,
        }
    }

    /// Drop the current JIT module and allocate a fresh runtime heap.
    pub fn reset(&mut self) {
        self.engine = Engine::with_link_profile(self.runtime_link_profile);
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
