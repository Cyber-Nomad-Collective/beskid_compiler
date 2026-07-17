//! Generation-safe identities for expanded syntax nodes.

/// Dense node identity within one expanded source-unit generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AstNodeId(pub u32);

/// Identity of one expanded syntax generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SyntaxGenerationId(pub u64);

/// Globally unambiguous key for one AST node in one source-unit generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AstNodeKey<UnitId> {
    pub unit: UnitId,
    pub generation: SyntaxGenerationId,
    pub node: AstNodeId,
}

impl<UnitId: Copy + Eq> AstNodeKey<UnitId> {
    /// Whether this key belongs to the supplied source unit and syntax generation.
    pub fn is_current(self, unit: UnitId, generation: SyntaxGenerationId) -> bool {
        self.unit == unit && self.generation == generation
    }
}
