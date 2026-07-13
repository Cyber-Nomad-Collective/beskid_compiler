//! Public AST/Salsa semantic contracts used by later frontend and codegen replacement slices.

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use beskid_analysis::projects::ProgramAssembly;
use beskid_analysis::syntax::SyntaxGenerationId;

use crate::db::Db;
use crate::inputs::ProjectSession;

/// Source-unit identity, interned by a normalized absolute logical path.
#[salsa::interned(constructor = intern_path, no_lifetime, debug)]
pub struct SourceUnitId {
    #[get(interned_path)]
    #[returns(ref)]
    path: PathBuf,
}

impl SourceUnitId {
    /// Normalize the deepest existing ancestor before interning the remaining logical suffix.
    ///
    /// This makes new LSP files stable when they are first named through a symlink and later
    /// created on disk.
    pub fn new(db: &dyn Db, path: PathBuf) -> Self {
        Self::intern_path(db, normalized_source_path(&path))
    }

    pub fn path(self, db: &dyn Db) -> &PathBuf {
        self.interned_path(db)
    }
}

fn normalized_source_path(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let logical = lexically_normalize(&absolute);
    let mut ancestor = logical.clone();
    let mut suffix = Vec::<OsString>::new();

    loop {
        if let Ok(mut canonical) = ancestor.canonicalize() {
            for component in suffix.iter().rev() {
                canonical.push(component);
            }
            return canonical;
        }
        let Some(leaf) = ancestor.file_name().map(ToOwned::to_owned) else {
            return logical;
        };
        suffix.push(leaf);
        if !ancestor.pop() {
            return logical;
        }
    }
}

fn lexically_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

/// Generation-safe key for a syntax node in an interned source unit.
pub type AstNodeKey = beskid_analysis::syntax::AstNodeKey<SourceUnitId>;

/// Typed frontend contract passed to later semantic consumers.
#[derive(Clone)]
pub struct TypedProgram {
    pub project: ProjectSession,
    pub entry: SourceUnitId,
    pub generation: SyntaxGenerationId,
    pub assembly: Arc<ProgramAssembly>,
}

/// Authoritative Salsa input for the current syntax generation of one source unit.
#[salsa::input]
pub struct SyntaxUnitInput {
    pub unit: SourceUnitId,
    pub generation: SyntaxGenerationId,
}

impl SyntaxUnitInput {
    /// Whether `key` belongs to this authoritative unit revision.
    pub fn accepts_key(self, db: &dyn Db, key: AstNodeKey) -> bool {
        key.is_current(self.unit(db), self.generation(db))
    }
}

/// Resolution fact for an item reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResolvedItem {
    pub declaration: AstNodeKey,
}

/// Resolution fact for a local reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResolvedLocal {
    pub declaration: AstNodeKey,
}

/// Opaque semantic type identity owned by the query layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SemanticTypeId(pub u32);

/// Backend-relevant call classification, detached from legacy HIR nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CallLowering {
    Direct(AstNodeKey),
    Dynamic,
    Runtime(RuntimeIntrinsic),
}

/// One semantic cast required while lowering an AST node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CastIntent {
    pub from: SemanticTypeId,
    pub to: SemanticTypeId,
}

/// Control-flow facts established for one AST node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ControlFlow {
    pub may_fall_through: bool,
}

/// Callable item signature expressed entirely in semantic type identities.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ItemSignature {
    pub parameters: Arc<[SemanticTypeId]>,
    pub result: SemanticTypeId,
}

/// Trusted runtime operation selected by semantic analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RuntimeIntrinsic(pub u32);

fn require_current(db: &dyn Db, syntax: SyntaxUnitInput, key: AstNodeKey) -> Option<()> {
    syntax.accepts_key(db, key).then_some(())
}

/// Declared for Task 2 semantic production; currently unavailable for current keys and rejects stale keys.
#[salsa::tracked]
pub fn resolved_item(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> Option<ResolvedItem> {
    require_current(db, syntax, key)?;
    None
}

/// Declared for Task 2 semantic production; currently unavailable for current keys and rejects stale keys.
#[salsa::tracked]
pub fn resolved_local(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> Option<ResolvedLocal> {
    require_current(db, syntax, key)?;
    None
}

/// Declared for Task 2 semantic production; currently unavailable for current keys and rejects stale keys.
#[salsa::tracked]
pub fn node_type(db: &dyn Db, syntax: SyntaxUnitInput, key: AstNodeKey) -> Option<SemanticTypeId> {
    require_current(db, syntax, key)?;
    None
}

/// Declared for Task 2 semantic production; currently unavailable for current keys and rejects stale keys.
#[salsa::tracked]
pub fn call_lowering(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> Option<CallLowering> {
    require_current(db, syntax, key)?;
    None
}

/// Declared for Task 2 semantic production; currently unavailable for current keys and rejects stale keys.
#[salsa::tracked]
pub fn cast_intents(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> Option<Arc<[CastIntent]>> {
    require_current(db, syntax, key)?;
    None
}

/// Declared for Task 2 semantic production; currently unavailable for current keys and rejects stale keys.
#[salsa::tracked]
pub fn control_flow(db: &dyn Db, syntax: SyntaxUnitInput, key: AstNodeKey) -> Option<ControlFlow> {
    require_current(db, syntax, key)?;
    None
}

/// Declared for Task 2 semantic production; currently unavailable for current keys and rejects stale keys.
#[salsa::tracked]
pub fn item_signature(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> Option<ItemSignature> {
    require_current(db, syntax, key)?;
    None
}

/// Declared for Task 2 trusted selection; there is deliberately no caller-injected intrinsic seam.
#[salsa::tracked]
pub fn runtime_intrinsic(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> Option<RuntimeIntrinsic> {
    require_current(db, syntax, key)?;
    None
}
