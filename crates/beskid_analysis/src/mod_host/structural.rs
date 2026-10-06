//! Generation-owned structural contributions. Opaque owned handles are lookup tokens,
//! never pointers dereferenced as Rust AST values and never generated source strings.
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::syntax::{Node, SpanInfo, Spanned};
use crate::syntax_query::DynNodeRef;

static NEXT_HANDLE: AtomicUsize = AtomicUsize::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructuralContributionTag {
    ContractDefinition,
    TypeDefinition,
    FunctionDefinition,
    ImplBlock,
}

/// Only a live arena issues identities. This is an owned token, never an address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StructuralNodeHandle {
    token: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StructuralContributionItem {
    pub tag: StructuralContributionTag,
    pub node: StructuralNodeHandle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuralProvenance {
    pub source_name: String,
    pub generator_type_id: String,
    pub origin_span: SpanInfo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StructuralContributionBounds {
    pub max_items: usize,
    pub max_nodes: usize,
    pub max_depth: usize,
}

impl Default for StructuralContributionBounds {
    fn default() -> Self {
        Self { max_items: 4096, max_nodes: 65536, max_depth: 128 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuralMaterializedItem {
    pub item: Spanned<Node>,
    pub provenance: StructuralProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StructuralContributionError {
    #[error("invalid structural arena owner, generation or bounds")]
    InvalidArena,
    #[error("structural contribution arena is closed")]
    Closed,
    #[error("structural contribution owner mismatch")]
    OwnerMismatch,
    #[error("structural contribution generation mismatch: expected {expected}, actual {actual}")]
    GenerationMismatch { expected: u64, actual: u64 },
    #[error("unknown or null structural contribution handle")]
    UnknownHandle,
    #[error("structural contribution handle kind mismatch")]
    KindMismatch,
    #[error("unsupported top-level structural contribution")]
    UnsupportedItem,
    #[error("structural contribution exceeds declared bounds")]
    BoundsExceeded,
    #[error("duplicate structural contribution handle")]
    DuplicateHandle,
    #[error("structural handle identity exhausted")]
    HandleExhausted,
    #[error("structural contribution arena lock is poisoned")]
    Poisoned,
}

#[derive(Debug, Default)]
struct ArenaState {
    closed: bool,
    total_nodes: usize,
    items: HashMap<usize, StructuralMaterializedItem>,
}

#[derive(Debug)]
pub struct StructuralContributionArena {
    owner: String,
    generation: u64,
    bounds: StructuralContributionBounds,
    state: Mutex<ArenaState>,
}

impl StructuralContributionArena {
    pub fn new(
        owner: String,
        generation: u64,
        bounds: StructuralContributionBounds,
    ) -> Result<Self, StructuralContributionError> {
        if owner.is_empty()
            || generation == 0
            || bounds.max_items == 0
            || bounds.max_nodes == 0
            || bounds.max_depth == 0
            || bounds.max_items > 4096
            || bounds.max_nodes > 65536
            || bounds.max_depth > 128
        {
            return Err(StructuralContributionError::InvalidArena);
        }
        Ok(Self { owner, generation, bounds, state: Mutex::new(ArenaState::default()) })
    }

    pub fn bounds(&self) -> StructuralContributionBounds {
        self.bounds
    }

    pub fn insert(
        &self,
        item: Spanned<Node>,
        provenance: StructuralProvenance,
    ) -> Result<StructuralContributionItem, StructuralContributionError> {
        if provenance.generator_type_id != self.owner {
            return Err(StructuralContributionError::OwnerMismatch);
        }
        let tag = contribution_tag(&item.node)?;
        let node_count = validate_tree(&item, self.bounds)?;
        let mut state = self.state.lock().map_err(|_| StructuralContributionError::Poisoned)?;
        if state.closed {
            return Err(StructuralContributionError::Closed);
        }
        if state.items.len() >= self.bounds.max_items {
            return Err(StructuralContributionError::BoundsExceeded);
        }
        let total_nodes = state
            .total_nodes
            .checked_add(node_count)
            .filter(|total| *total <= self.bounds.max_nodes)
            .ok_or(StructuralContributionError::BoundsExceeded)?;
        // Process-wide, checked monotonic IDs cannot be reused after arena close/drop.
        // The wire field remains opaque: no address allocation or AST pointer cast occurs.
        let token = NEXT_HANDLE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| value.checked_add(1))
            .map_err(|_| StructuralContributionError::HandleExhausted)?;
        state.items.insert(token, StructuralMaterializedItem { item, provenance });
        state.total_nodes = total_nodes;
        Ok(StructuralContributionItem { tag, node: StructuralNodeHandle { token } })
    }

    pub fn materialize(
        &self,
        contributions: &[StructuralContributionItem],
        owner: &str,
        generation: u64,
    ) -> Result<Vec<StructuralMaterializedItem>, StructuralContributionError> {
        if owner != self.owner {
            return Err(StructuralContributionError::OwnerMismatch);
        }
        if generation != self.generation {
            return Err(StructuralContributionError::GenerationMismatch {
                expected: self.generation,
                actual: generation,
            });
        }
        if contributions.len() > self.bounds.max_items {
            return Err(StructuralContributionError::BoundsExceeded);
        }
        let state = self.state.lock().map_err(|_| StructuralContributionError::Poisoned)?;
        if state.closed {
            return Err(StructuralContributionError::Closed);
        }
        let mut seen = HashSet::new();
        let mut materialized = Vec::with_capacity(contributions.len());
        for contribution in contributions {
            let token = contribution.node.token;
            let item = state.items.get(&token).ok_or(StructuralContributionError::UnknownHandle)?;
            if !seen.insert(token) {
                return Err(StructuralContributionError::DuplicateHandle);
            }
            if contribution.tag != contribution_tag(&item.item.node)? {
                return Err(StructuralContributionError::KindMismatch);
            }
            materialized.push(item.clone());
        }
        Ok(materialized)
    }

    pub fn close(&self) -> Result<(), StructuralContributionError> {
        let mut state = self.state.lock().map_err(|_| StructuralContributionError::Poisoned)?;
        state.closed = true;
        state.items.clear();
        state.total_nodes = 0;
        Ok(())
    }
}

fn contribution_tag(node: &Node) -> Result<StructuralContributionTag, StructuralContributionError> {
    match node {
        Node::Function(_) => Ok(StructuralContributionTag::FunctionDefinition),
        Node::TypeDefinition(_) => Ok(StructuralContributionTag::TypeDefinition),
        Node::ContractDefinition(_) => Ok(StructuralContributionTag::ContractDefinition),
        Node::ImplBlock(_) => Ok(StructuralContributionTag::ImplBlock),
        _ => Err(StructuralContributionError::UnsupportedItem),
    }
}

fn validate_tree(
    item: &Spanned<Node>,
    bounds: StructuralContributionBounds,
) -> Result<usize, StructuralContributionError> {
    let mut pending = vec![(DynNodeRef::from(item), 1usize)];
    let mut visited = 0usize;
    while let Some((node, depth)) = pending.pop() {
        visited += 1;
        if visited > bounds.max_nodes || depth > bounds.max_depth {
            return Err(StructuralContributionError::BoundsExceeded);
        }
        let mut exceeded = false;
        node.children(|child| {
            if visited + pending.len() >= bounds.max_nodes || depth >= bounds.max_depth {
                exceeded = true;
            } else {
                pending.push((child, depth + 1));
            }
        });
        if exceeded {
            return Err(StructuralContributionError::BoundsExceeded);
        }
    }
    Ok(visited)
}
