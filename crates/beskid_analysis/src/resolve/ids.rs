//! Opaque dense indices assigned during resolution.

/// Node in the hierarchical module tree built from `module` / `inline module` items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ModuleId(pub usize);

/// Top-level or nested declaration (including synthetic items such as parameters).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ItemId(pub usize);

/// Function-local binding introduced in a scope stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct LocalId(pub usize);

/// Dense stable id for typable HIR nodes (expressions, statements, patterns).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub struct HirNodeId(pub u32);

impl HirNodeId {
    pub const INVALID: Self = Self(0);
    #[must_use]
    pub fn is_valid(self) -> bool {
        self != Self::INVALID
    }
}
