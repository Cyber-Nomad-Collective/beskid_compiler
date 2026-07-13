//! Public AST/Salsa semantic contracts used by later frontend and codegen replacement slices.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use beskid_analysis::projects::ProgramAssembly;
use beskid_analysis::syntax::SyntaxGenerationId;

use crate::db::Db;
use crate::inputs::ProjectSession;

/// Canonical source-unit identity, interned by its path.
#[salsa::interned(constructor = intern_path, no_lifetime, debug)]
pub struct SourceUnitId {
    #[get(interned_path)]
    #[returns(ref)]
    path: PathBuf,
}

impl SourceUnitId {
    /// Intern a physical source path after canonicalizing aliases such as `.` and symlinks.
    pub fn new(db: &dyn Db, path: PathBuf) -> Self {
        Self::intern_path(db, canonical_source_path(&path))
    }

    pub fn path(self, db: &dyn Db) -> &PathBuf {
        self.interned_path(db)
    }
}

fn canonical_source_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(path)
        };
        let mut normalized = PathBuf::new();
        for component in absolute.components() {
            match component {
                Component::CurDir => {}
                Component::ParentDir => {
                    normalized.pop();
                }
                other => normalized.push(other.as_os_str()),
            }
        }
        normalized
    })
}

/// Generation-safe key for a syntax node in an interned source unit.
pub type AstNodeKey = beskid_analysis::syntax::AstNodeKey<SourceUnitId>;

/// Typed frontend contract passed to AST semantic queries.
#[derive(Clone)]
pub struct TypedProgram {
    pub project: ProjectSession,
    pub entry: SourceUnitId,
    pub generation: SyntaxGenerationId,
    pub assembly: Arc<ProgramAssembly>,
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

/// Immutable semantic facts supplied to the public tracked-query contract.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SemanticFacts {
    pub resolved_items: HashMap<AstNodeKey, ResolvedItem>,
    pub resolved_locals: HashMap<AstNodeKey, ResolvedLocal>,
    pub node_types: HashMap<AstNodeKey, SemanticTypeId>,
    pub call_lowerings: HashMap<AstNodeKey, CallLowering>,
    pub cast_intents: HashMap<AstNodeKey, Arc<[CastIntent]>>,
    pub control_flow: HashMap<AstNodeKey, ControlFlow>,
    pub item_signatures: HashMap<AstNodeKey, ItemSignature>,
    pub runtime_intrinsics: HashMap<AstNodeKey, RuntimeIntrinsic>,
}

/// Salsa revision input for one immutable semantic-fact snapshot.
#[salsa::input]
pub struct SemanticFactsInput {
    #[returns(ref)]
    pub facts: Arc<SemanticFacts>,
}

fn accepts_key(entry: SourceUnitId, generation: SyntaxGenerationId, key: AstNodeKey) -> bool {
    key.is_current(entry, generation)
}

#[salsa::tracked]
pub fn resolved_item(
    db: &dyn Db,
    facts: SemanticFactsInput,
    entry: SourceUnitId,
    generation: SyntaxGenerationId,
    key: AstNodeKey,
) -> Option<ResolvedItem> {
    accepts_key(entry, generation, key)
        .then(|| facts.facts(db).resolved_items.get(&key).copied())
        .flatten()
}

#[salsa::tracked]
pub fn resolved_local(
    db: &dyn Db,
    facts: SemanticFactsInput,
    entry: SourceUnitId,
    generation: SyntaxGenerationId,
    key: AstNodeKey,
) -> Option<ResolvedLocal> {
    accepts_key(entry, generation, key)
        .then(|| facts.facts(db).resolved_locals.get(&key).copied())
        .flatten()
}

#[salsa::tracked]
pub fn node_type(
    db: &dyn Db,
    facts: SemanticFactsInput,
    entry: SourceUnitId,
    generation: SyntaxGenerationId,
    key: AstNodeKey,
) -> Option<SemanticTypeId> {
    accepts_key(entry, generation, key)
        .then(|| facts.facts(db).node_types.get(&key).copied())
        .flatten()
}

#[salsa::tracked]
pub fn call_lowering(
    db: &dyn Db,
    facts: SemanticFactsInput,
    entry: SourceUnitId,
    generation: SyntaxGenerationId,
    key: AstNodeKey,
) -> Option<CallLowering> {
    accepts_key(entry, generation, key)
        .then(|| facts.facts(db).call_lowerings.get(&key).copied())
        .flatten()
}

#[salsa::tracked]
pub fn cast_intents(
    db: &dyn Db,
    facts: SemanticFactsInput,
    entry: SourceUnitId,
    generation: SyntaxGenerationId,
    key: AstNodeKey,
) -> Option<Arc<[CastIntent]>> {
    accepts_key(entry, generation, key)
        .then(|| facts.facts(db).cast_intents.get(&key).cloned())
        .flatten()
}

#[salsa::tracked]
pub fn control_flow(
    db: &dyn Db,
    facts: SemanticFactsInput,
    entry: SourceUnitId,
    generation: SyntaxGenerationId,
    key: AstNodeKey,
) -> Option<ControlFlow> {
    accepts_key(entry, generation, key)
        .then(|| facts.facts(db).control_flow.get(&key).copied())
        .flatten()
}

#[salsa::tracked]
pub fn item_signature(
    db: &dyn Db,
    facts: SemanticFactsInput,
    entry: SourceUnitId,
    generation: SyntaxGenerationId,
    key: AstNodeKey,
) -> Option<ItemSignature> {
    accepts_key(entry, generation, key)
        .then(|| facts.facts(db).item_signatures.get(&key).cloned())
        .flatten()
}

#[salsa::tracked]
pub fn runtime_intrinsic(
    db: &dyn Db,
    facts: SemanticFactsInput,
    entry: SourceUnitId,
    generation: SyntaxGenerationId,
    key: AstNodeKey,
) -> Option<RuntimeIntrinsic> {
    accepts_key(entry, generation, key)
        .then(|| facts.facts(db).runtime_intrinsics.get(&key).copied())
        .flatten()
}
