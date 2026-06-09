//! Dense node index with parent links for [`DynNodeRef`] navigation (Mod SDK / host bridge).

use std::collections::HashMap;

use crate::syntax_query::{DynNodeRef, NodeKind};
use crate::syntax::{Program, SpanInfo, Spanned};

/// Stable handle matching Mod SDK `NodeRef` (`syntaxGenerationId` + `nodeId`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SyntaxNodeId {
    pub generation_id: u64,
    pub node_id: u32,
}

/// One indexed syntax node in a snapshot generation.
#[derive(Clone, Copy)]
struct SnapshotEntry<'a> {
    node: DynNodeRef<'a>,
    parent_id: Option<u32>,
    kind: NodeKind,
    span: Option<SpanInfo>,
}

/// Immutable index over a syntax tree: parent links, kinds, and `DynNodeRef` lookup.
pub struct SyntaxSnapshot<'a> {
    generation_id: u64,
    root_id: u32,
    entries: Vec<SnapshotEntry<'a>>,
    ptr_to_id: HashMap<usize, u32>,
}

impl<'a> SyntaxSnapshot<'a> {
    /// Build a snapshot from `program`, assigning dense `node_id` values in pre-order.
    pub fn from_program(program: &'a Spanned<Program>, generation_id: u64) -> Self {
        let root = DynNodeRef::from(&program.node);
        Self::from_root(root, generation_id)
    }

    /// Build a snapshot rooted at any syntax node.
    pub fn from_root(root: DynNodeRef<'a>, generation_id: u64) -> Self {
        let mut entries = Vec::new();
        let mut ptr_to_id = HashMap::new();
        index_subtree(root, None, &mut entries, &mut ptr_to_id);
        let root_id = ptr_to_id
            .get(&node_ptr(root))
            .copied()
            .expect("root indexed");
        Self {
            generation_id,
            root_id,
            entries,
            ptr_to_id,
        }
    }

    pub fn generation_id(&self) -> u64 {
        self.generation_id
    }

    pub fn root_id(&self) -> u32 {
        self.root_id
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn id_of(&self, node: DynNodeRef<'a>) -> Option<u32> {
        self.ptr_to_id.get(&node_ptr(node)).copied()
    }

    pub fn stable_id(&self, node: DynNodeRef<'a>) -> Option<SyntaxNodeId> {
        self.id_of(node).map(|node_id| SyntaxNodeId {
            generation_id: self.generation_id,
            node_id,
        })
    }

    pub fn node_at(&self, node_id: u32) -> Option<DynNodeRef<'a>> {
        self.entries.get(node_id as usize).map(|e| e.node)
    }

    pub fn kind_of(&self, node_id: u32) -> Option<NodeKind> {
        self.entries.get(node_id as usize).map(|e| e.kind)
    }

    pub fn span_of(&self, node_id: u32) -> Option<SpanInfo> {
        self.entries.get(node_id as usize)?.span
    }

    pub fn parent_id(&self, node_id: u32) -> Option<u32> {
        self.entries.get(node_id as usize)?.parent_id
    }

    pub fn parent_of(&self, node: DynNodeRef<'a>) -> Option<DynNodeRef<'a>> {
        let id = self.id_of(node)?;
        let parent_id = self.parent_id(id)?;
        self.node_at(parent_id)
    }

    pub fn resolve(&self, stable: SyntaxNodeId) -> Option<DynNodeRef<'a>> {
        if stable.generation_id != self.generation_id {
            return None;
        }
        self.node_at(stable.node_id)
    }

    pub fn resolve_span(&self, stable: SyntaxNodeId) -> Option<SpanInfo> {
        if stable.generation_id != self.generation_id {
            return None;
        }
        self.span_of(stable.node_id)
    }
}

fn node_ptr(node: DynNodeRef<'_>) -> usize {
    std::ptr::from_ref(node.0).addr()
}

fn index_subtree<'a>(
    node: DynNodeRef<'a>,
    parent_id: Option<u32>,
    entries: &mut Vec<SnapshotEntry<'a>>,
    ptr_to_id: &mut HashMap<usize, u32>,
) {
    let node_id = entries.len() as u32;
    ptr_to_id.insert(node_ptr(node), node_id);
    entries.push(SnapshotEntry {
        node,
        parent_id,
        kind: node.node_kind(),
        span: node.span(),
    });
    node.children(|child| index_subtree(child, Some(node_id), entries, ptr_to_id));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::parse_program;

    #[test]
    fn snapshot_parent_chain_from_program() {
        let src = r#"
            pub fn outer() {
                let x = 1;
            }
        "#;
        let program = parse_program(src).expect("parse");
        let snap = SyntaxSnapshot::from_program(&program, 1);
        assert!(snap.len() > 1);

        let mut func_id = None;
        for id in 0..snap.len() as u32 {
            if snap.kind_of(id) == Some(NodeKind::FunctionDefinition) {
                func_id = Some(id);
                break;
            }
        }
        let func_id = func_id.expect("function indexed");
        let parent_id = snap.parent_id(func_id).expect("func has parent");
        let parent = snap.node_at(parent_id).expect("parent node");
        assert_eq!(parent.node_kind(), NodeKind::Node);
    }
}
