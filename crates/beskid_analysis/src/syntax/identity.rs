//! Generation-safe identities for expanded syntax nodes.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

/// Dense node identity within one expanded source-unit generation.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct AstNodeId(pub u32);

impl AstNodeId {
    pub const INVALID: Self = Self(u32::MAX);

    #[must_use]
    pub fn is_valid(self) -> bool {
        self != Self::INVALID
    }
}

impl Default for AstNodeId {
    fn default() -> Self {
        Self::INVALID
    }
}

impl fmt::Debug for AstNodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "n{}", self.0)
    }
}

impl fmt::Display for AstNodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "n{}", self.0)
    }
}

/// Identity of one expanded syntax generation.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct SyntaxGenerationId(pub u64);

static LAST_SYNTAX_GENERATION: AtomicU64 = AtomicU64::new(0);

impl SyntaxGenerationId {
    /// Allocate after every generation constructed or successfully restored in
    /// this process. Exhaustion is an error, never a wrap or a reused identity.
    pub fn allocate() -> Option<Self> {
        LAST_SYNTAX_GENERATION
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |last| last.checked_add(1))
            .ok()
            .map(|last| Self(last + 1))
    }

    /// Observe existing syntax authority before allocating any newer assembly.
    /// Snapshot readers call this only after the whole candidate is accepted;
    /// merely deserializing an untrusted candidate must not advance allocation.
    pub fn resume_after(self) {
        LAST_SYNTAX_GENERATION.fetch_max(self.0, Ordering::Relaxed);
    }
}

impl fmt::Debug for SyntaxGenerationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "g{}", self.0)
    }
}

impl fmt::Display for SyntaxGenerationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "g{}", self.0)
    }
}

/// Globally unambiguous key for one AST node in one source-unit generation.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct AstNodeKey<UnitId> {
    pub unit: UnitId,
    pub generation: SyntaxGenerationId,
    pub node: AstNodeId,
}

impl<UnitId> AstNodeKey<UnitId> {
    /// Generation/node cursor used in traces when the unit path is shown separately.
    pub fn cursor_label(&self) -> String {
        format!("{}:{}", self.generation, self.node)
    }

    /// Full `path#gN:nN` label used in compiler traces and diagnostics.
    pub fn display_label(&self, unit_path: impl fmt::Display) -> String {
        format!("{unit_path}#{}", self.cursor_label())
    }
}

impl<UnitId: Copy + Eq> AstNodeKey<UnitId> {
    /// Whether this key belongs to the supplied source unit and syntax generation.
    pub fn is_current(self, unit: UnitId, generation: SyntaxGenerationId) -> bool {
        self.unit == unit && self.generation == generation
    }
}

impl<UnitId: fmt::Debug> fmt::Debug for AstNodeKey<UnitId> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Prefer the shared #gN:nN cursor over Debug-dumping opaque unit ids (e.g. salsa Id(N)).
        write!(f, "#{}", self.cursor_label())
    }
}

impl<UnitId: fmt::Display> fmt::Display for AstNodeKey<UnitId> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#{}", self.unit, self.cursor_label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocation_resumes_after_constructed_syntax_without_regressing() {
        let first = SyntaxGenerationId::allocate().unwrap();
        let observed = SyntaxGenerationId(first.0 + 100);
        let program = crate::services::parse_program("i64 Main() { return 1_i64; }").unwrap();
        crate::syntax_query::SyntaxIndex::from_program(&program, observed);
        first.resume_after();
        let next = SyntaxGenerationId::allocate().unwrap();
        assert!(next > observed);
        assert!(SyntaxGenerationId::allocate().unwrap() > next);
    }

    #[test]
    fn formats_generation_and_node_cursors() {
        assert_eq!(AstNodeId(21).to_string(), "n21");
        assert_eq!(SyntaxGenerationId(96).to_string(), "g96");
        assert_eq!(format!("{:?}", AstNodeId(21)), "n21");
        assert_eq!(format!("{:?}", SyntaxGenerationId(96)), "g96");
    }

    #[test]
    fn formats_ast_node_key_labels() {
        let key = AstNodeKey { unit: "/tmp/String.bd", generation: SyntaxGenerationId(96), node: AstNodeId(21) };
        assert_eq!(key.cursor_label(), "g96:n21");
        assert_eq!(key.display_label("/tmp/String.bd"), "/tmp/String.bd#g96:n21");
        assert_eq!(format!("{key}"), "/tmp/String.bd#g96:n21");
        assert_eq!(format!("{key:?}"), "#g96:n21");
    }
}
