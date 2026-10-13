//! Owned metadata index for one expanded syntax generation.

use crate::syntax::{AstNodeId, Program, SpanInfo, Spanned, SyntaxGenerationId};
use crate::syntax_query::{DynNodeRef, NodeKind};

/// Stable metadata for one node in deterministic syntax pre-order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SyntaxNodeMetadata {
    pub id: AstNodeId,
    pub parent: Option<AstNodeId>,
    pub kind: NodeKind,
    pub span: Option<SpanInfo>,
}

/// Generation-bound, pointer-free index over an expanded syntax tree.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SyntaxIndex {
    generation: SyntaxGenerationId,
    metadata: Vec<SyntaxNodeMetadata>,
    children: Vec<Vec<AstNodeId>>,
    paths: Vec<Vec<u32>>,
    /// Node ids per `NodeKind` (indexed by discriminant) in pre-order, so kind lookups visit only
    /// the matching nodes instead of the whole unit.
    kinds: Vec<Vec<AstNodeId>>,
    /// Declaration ids per `NodeKind` and declared name (types, enums, contracts, functions,
    /// methods), in pre-order: the unit's name table for declaration lookups.
    names: Vec<std::collections::BTreeMap<String, Vec<AstNodeId>>>,
}

impl SyntaxIndex {
    /// Index `program` in deterministic pre-order after expansion has completed.
    pub fn from_program(program: &Spanned<Program>, generation: SyntaxGenerationId) -> Self {
        generation.resume_after();
        let mut metadata = Vec::new();
        let mut children = Vec::new();
        let mut paths = Vec::new();
        // The root is indexed as the spanned program so its metadata carries the unit span like
        // every other node; the bare `Program` has no span and would leave `Query.Span(entryRoot)`
        // unanswerable while every child answers.
        index_node(DynNodeRef::from(program), None, &[], &mut metadata, &mut children, &mut paths);
        let mut kinds = vec![Vec::new(); NodeKind::ALL.len()];
        for node in &metadata {
            kinds[node.kind as usize].push(node.id);
        }
        let mut index = Self { generation, metadata, children, paths, kinds, names: Vec::new() };
        let mut names = vec![std::collections::BTreeMap::<String, Vec<AstNodeId>>::new(); NodeKind::ALL.len()];
        for kind in NAMED_DECLARATION_KINDS {
            for id in index.ids_of_kind(kind) {
                if let Some(name) = index.node_at(program, id).and_then(declared_name) {
                    names[kind as usize].entry(name.to_owned()).or_default().push(id);
                }
            }
        }
        index.names = names;
        index
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
        self.kinds.get(kind as usize).into_iter().flatten().copied()
    }

    /// Declarations of `kind` named `name`, in pre-order. Answers only for the declaration kinds
    /// that carry a name (types, enums, contracts, functions, methods); empty for any other kind.
    pub fn ids_named<'a>(&'a self, kind: NodeKind, name: &str) -> impl Iterator<Item = AstNodeId> + 'a {
        self.names.get(kind as usize).and_then(|names| names.get(name)).into_iter().flatten().copied()
    }

    /// Direct children in deterministic AST order.
    pub fn children(&self, id: AstNodeId) -> Option<&[AstNodeId]> {
        self.children.get(id.0 as usize).map(Vec::as_slice)
    }

    /// Resolve a node through its indexed child path without rebuilding a whole-tree snapshot.
    pub fn node_at<'a>(&self, program: &'a Spanned<Program>, id: AstNodeId) -> Option<DynNodeRef<'a>> {
        let path = self.paths.get(id.0 as usize)?;
        let mut node = DynNodeRef::from(program);
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

/// Declaration kinds indexed by name in [`SyntaxIndex::ids_named`].
const NAMED_DECLARATION_KINDS: [NodeKind; 5] = [
    NodeKind::TypeDefinition,
    NodeKind::EnumDefinition,
    NodeKind::ContractDefinition,
    NodeKind::FunctionDefinition,
    NodeKind::MethodDefinition,
];

fn declared_name(node: DynNodeRef<'_>) -> Option<&str> {
    use crate::syntax::{ContractDefinition, EnumDefinition, FunctionDefinition, MethodDefinition, TypeDefinition};
    node.of::<TypeDefinition>()
        .map(|definition| definition.name.node.name.as_str())
        .or_else(|| node.of::<EnumDefinition>().map(|definition| definition.name.node.name.as_str()))
        .or_else(|| node.of::<ContractDefinition>().map(|definition| definition.name.node.name.as_str()))
        .or_else(|| node.of::<FunctionDefinition>().map(|definition| definition.name.node.name.as_str()))
        .or_else(|| node.of::<MethodDefinition>().map(|definition| definition.name.node.name.as_str()))
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
