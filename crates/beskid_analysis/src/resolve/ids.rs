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
