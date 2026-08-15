//! Owned metadata index for one expanded syntax generation.

use crate::syntax::{AstNodeId, Program, SpanInfo, Spanned, SyntaxGenerationId};
use crate::syntax_query::{DynNodeRef, NodeKind};

/// Stable metadata for one node in deterministic syntax pre-order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntaxNodeMetadata {
    pub id: AstNodeId,
    pub parent: Option<AstNodeId>,
    pub kind: NodeKind,
    pub span: Option<SpanInfo>,
}

/// Generation-bound, pointer-free index over an expanded syntax tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxIndex {
    generation: SyntaxGenerationId,
    metadata: Vec<SyntaxNodeMetadata>,
    children: Vec<Vec<AstNodeId>>,
    paths: Vec<Vec<u32>>,
}

impl SyntaxIndex {
    /// Index `program` in deterministic pre-order after expansion has completed.
    pub fn from_program(program: &Spanned<Program>, generation: SyntaxGenerationId) -> Self {
        let mut metadata = Vec::new();
        let mut children = Vec::new();
        let mut paths = Vec::new();
        index_node(DynNodeRef::from(&program.node), None, &[], &mut metadata, &mut children, &mut paths);
        Self { generation, metadata, children, paths }
    }

    pub fn generation(&self) -> SyntaxGenerationId {
        self.generation
    }

    pub fn len(&self) -> usize {
        self.metadata.len()
    }

    pub fn is_empty(&self) -> bool {
        self.metadata.is_empty()
    }

    pub fn metadata(&self) -> &[SyntaxNodeMetadata] {
        &self.metadata
    }

    pub fn kind(&self, id: AstNodeId) -> Option<NodeKind> {
        self.metadata.get(id.0 as usize).map(|node| node.kind)
    }

    pub fn ids_of_kind(&self, kind: NodeKind) -> impl Iterator<Item = AstNodeId> + '_ {
        self.metadata.iter().filter(move |node| node.kind == kind).map(|node| node.id)
    }

    /// Direct children in deterministic AST order.
    pub fn children(&self, id: AstNodeId) -> Option<&[AstNodeId]> {
        self.children.get(id.0 as usize).map(Vec::as_slice)
    }

    /// Resolve a node through its indexed child path without rebuilding a whole-tree snapshot.
    pub fn node_at<'a>(&self, program: &'a Spanned<Program>, id: AstNodeId) -> Option<DynNodeRef<'a>> {
        let path = self.paths.get(id.0 as usize)?;
        let mut node = DynNodeRef::from(&program.node);
        for ordinal in path {
            node = direct_child(node, *ordinal)?;
        }
        Some(node)
    }

    /// Resolve the exact already-indexed direct child represented by `target`.
    pub fn direct_child_id(
        &self,
        program: &Spanned<Program>,
        parent: AstNodeId,
        target: DynNodeRef<'_>,
    ) -> Option<AstNodeId> {
        self.children(parent)?.iter().copied().find(|child| {
            self.node_at(program, *child).is_some_and(|node| {
                std::ptr::eq(std::ptr::from_ref(node.0).cast::<()>(), std::ptr::from_ref(target.0).cast::<()>())
            })
        })
    }

    /// Resolve metadata only when the caller's generation is current.
    pub fn metadata_for(&self, generation: SyntaxGenerationId, id: AstNodeId) -> Option<&SyntaxNodeMetadata> {
        (generation == self.generation).then(|| self.metadata.get(id.0 as usize)).flatten()
    }

    /// Expanded `use` paths in deterministic syntax order.
    pub fn import_paths(&self, program: &Spanned<Program>) -> Vec<Vec<String>> {
        self.nodes_of_kind::<crate::syntax::UseDeclaration>(program, NodeKind::UseDeclaration)
            .map(|declaration| path_segments(&declaration.path.node))
            .collect()
    }

    /// Expanded out-of-line module declaration paths in deterministic syntax order.
    pub fn module_declaration_paths(&self, program: &Spanned<Program>) -> Vec<Vec<String>> {
        self.nodes_of_kind::<crate::syntax::ModuleDeclaration>(program, NodeKind::ModuleDeclaration)
            .map(|declaration| path_segments(&declaration.path.node))
            .collect()
    }

    /// Expanded inline-module names in deterministic syntax order.
    pub fn inline_module_names(&self, program: &Spanned<Program>) -> Vec<String> {
        self.nodes_of_kind::<crate::syntax::InlineModule>(program, NodeKind::InlineModule)
            .map(|module| module.name.node.name.clone())
            .collect()
    }

    fn nodes_of_kind<'a, T: crate::syntax_query::AstNode + 'static>(
        &'a self,
        program: &'a Spanned<Program>,
        kind: NodeKind,
    ) -> impl Iterator<Item = &'a T> + 'a {
        self.ids_of_kind(kind).filter_map(|id| self.node_at(program, id)?.of::<T>())
    }
}

fn path_segments(path: &crate::syntax::Path) -> Vec<String> {
    path.segments.iter().map(|segment| segment.node.name.node.name.clone()).collect()
}

fn index_node(
    node: DynNodeRef<'_>,
    parent: Option<AstNodeId>,
    path: &[u32],
    metadata: &mut Vec<SyntaxNodeMetadata>,
    children: &mut Vec<Vec<AstNodeId>>,
    paths: &mut Vec<Vec<u32>>,
) {
    let id = AstNodeId(u32::try_from(metadata.len()).expect("syntax node count exceeds u32"));
    metadata.push(SyntaxNodeMetadata { id, parent, kind: node.node_kind(), span: node.span() });
    children.push(Vec::new());
    paths.push(path.to_vec());
    let mut ordinal = 0u32;
    node.children(|child| {
        let child_id = AstNodeId(u32::try_from(metadata.len()).expect("syntax node count exceeds u32"));
        children[id.0 as usize].push(child_id);
        let mut child_path = path.to_vec();
        child_path.push(ordinal);
        ordinal += 1;
        index_node(child, Some(id), &child_path, metadata, children, paths);
    });
}

fn direct_child(node: DynNodeRef<'_>, target: u32) -> Option<DynNodeRef<'_>> {
    let mut ordinal = 0u32;
    let mut result = None;
    node.children(|child| {
        if ordinal == target {
            result = Some(child);
        }
        ordinal += 1;
    });
    result
}
