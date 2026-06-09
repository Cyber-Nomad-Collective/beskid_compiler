//! Host bridge for `Beskid.Compiler.Query` — maps Mod SDK `NodeRef` to `beskid_analysis::syntax_query`.

use crate::syntax_query::{
    Ancestors, Descendants, DynNodeRef, NodeKind, Query, SyntaxNodeId, SyntaxSnapshot,
};
use crate::syntax::{Program, SpanInfo, Spanned};

/// Mod SDK `NodeRef` wire shape (`Beskid.Syntax.Nodes.NodeRef`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SdkNodeRef {
    pub syntax_generation_id: u64,
    pub node_id: u32,
}

/// Mod SDK `NodeSpan` wire shape (`Beskid.Syntax.Nodes.NodeSpan`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SdkNodeSpan {
    pub start: u64,
    pub end: u64,
    pub line_start: u64,
    pub column_start: u64,
    pub line_end: u64,
    pub column_end: u64,
}

impl From<SpanInfo> for SdkNodeSpan {
    fn from(value: SpanInfo) -> Self {
        Self {
            start: value.start as u64,
            end: value.end as u64,
            line_start: value.line_col_start.0 as u64,
            column_start: value.line_col_start.1 as u64,
            line_end: value.line_col_end.0 as u64,
            column_end: value.line_col_end.1 as u64,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryBounds {
    pub max_nodes: u64,
    pub max_depth: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdkSyntaxSelection {
    pub nodes: Vec<SdkNodeRef>,
    pub bounds: QueryBounds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineOpKind {
    Replace,
    Remove,
    InsertBefore,
    InsertAfter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PipelineOp {
    pub kind: PipelineOpKind,
    pub target: SdkNodeRef,
    pub payload: Option<SdkNodeRef>,
}

#[derive(Clone)]
pub struct SdkSyntaxPipeline<'a> {
    snapshot: &'a SyntaxSnapshot<'a>,
    pub root: SdkNodeRef,
    pub bounds: QueryBounds,
    ops: Vec<PipelineOp>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineValidationError {
    StaleGeneration { expected: u64, actual: u64 },
    MissingNode { node: SdkNodeRef },
    Conflict { target: SdkNodeRef },
}

impl SdkNodeRef {
    pub fn from_stable(id: SyntaxNodeId) -> Self {
        Self {
            syntax_generation_id: id.generation_id,
            node_id: id.node_id,
        }
    }

    pub fn to_stable(self) -> SyntaxNodeId {
        SyntaxNodeId {
            generation_id: self.syntax_generation_id,
            node_id: self.node_id,
        }
    }
}

/// Bounded query cursor over a materialized syntax snapshot.
pub struct SdkSyntaxQuery<'a> {
    snapshot: &'a SyntaxSnapshot<'a>,
    start: SdkNodeRef,
    max_nodes: u64,
    max_depth: u64,
    nodes_visited: u64,
    bound_exceeded: bool,
}

impl<'a> SdkSyntaxQuery<'a> {
    pub fn at(snapshot: &'a SyntaxSnapshot<'a>, root: SdkNodeRef) -> Self {
        Self {
            snapshot,
            start: root,
            max_nodes: 0,
            max_depth: 0,
            nodes_visited: 0,
            bound_exceeded: false,
        }
    }

    pub fn at_program(snapshot: &'a SyntaxSnapshot<'a>, program: &'a Spanned<Program>) -> Self {
        let root = snapshot
            .stable_id(DynNodeRef::from(&program.node))
            .map(SdkNodeRef::from_stable)
            .expect("program root must be indexed");
        Self::at(snapshot, root)
    }

    pub fn with_bounds(mut self, max_nodes: u64, max_depth: u64) -> Self {
        self.max_nodes = max_nodes;
        self.max_depth = max_depth;
        self
    }

    pub fn bounds_exceeded(&self) -> bool {
        self.bound_exceeded
    }

    pub fn bounds(&self) -> QueryBounds {
        QueryBounds {
            max_nodes: self.max_nodes,
            max_depth: self.max_depth,
        }
    }

    fn resolve(&self, id: SdkNodeRef) -> Option<DynNodeRef<'a>> {
        self.snapshot.resolve(id.to_stable())
    }

    pub fn descendants(&mut self) -> Vec<SdkNodeRef> {
        let Some(start) = self.resolve(self.start) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for node in Descendants::new(start) {
            if self.max_nodes > 0 && self.nodes_visited >= self.max_nodes {
                self.bound_exceeded = true;
                break;
            }
            if let Some(stable) = self.snapshot.stable_id(node) {
                out.push(SdkNodeRef::from_stable(stable));
            }
            self.nodes_visited += 1;
        }
        out
    }

    pub fn children(&self, node: SdkNodeRef) -> Vec<SdkNodeRef> {
        let Some(n) = self.resolve(node) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        n.children(|child| {
            if let Some(stable) = self.snapshot.stable_id(child) {
                out.push(SdkNodeRef::from_stable(stable));
            }
        });
        out
    }

    pub fn parent(&self, node: SdkNodeRef) -> Option<SdkNodeRef> {
        let stable = node.to_stable();
        let parent_id = self.snapshot.parent_id(stable.node_id)?;
        Some(SdkNodeRef::from_stable(SyntaxNodeId {
            generation_id: stable.generation_id,
            node_id: parent_id,
        }))
    }

    pub fn ancestors(&self, node: SdkNodeRef) -> Vec<SdkNodeRef> {
        let Some(start) = self.resolve(node) else {
            return Vec::new();
        };
        Ancestors::new(self.snapshot, start)
            .filter_map(|n| self.snapshot.stable_id(n).map(SdkNodeRef::from_stable))
            .collect()
    }

    pub fn of_kind(&mut self, kind: NodeKind) -> Vec<SdkNodeRef> {
        let Some(start) = self.resolve(self.start) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for node in Descendants::new(start) {
            if node.node_kind() != kind {
                continue;
            }
            if let Some(stable) = self.snapshot.stable_id(node) {
                out.push(SdkNodeRef::from_stable(stable));
            }
        }
        out
    }

    pub fn find_first(&mut self, kind: NodeKind) -> Option<SdkNodeRef> {
        self.of_kind(kind).into_iter().next()
    }

    pub fn find_first_typed<T: crate::syntax_query::AstNode + 'static>(&mut self) -> Option<SdkNodeRef> {
        let start = self.resolve(self.start)?;
        for node in Descendants::new(start) {
            if node.of::<T>().is_none() {
                continue;
            }
            return self.snapshot.stable_id(node).map(SdkNodeRef::from_stable);
        }
        None
    }

    pub fn span(&self, node: SdkNodeRef) -> Option<SdkNodeSpan> {
        self.snapshot.resolve_span(node.to_stable()).map(Into::into)
    }

    pub fn try_span(&self, node: SdkNodeRef) -> Option<SdkNodeSpan> {
        self.span(node)
    }

    pub fn select(&mut self) -> SdkSyntaxSelection {
        SdkSyntaxSelection {
            nodes: self.descendants(),
            bounds: self.bounds(),
        }
    }

    pub fn where_kind(&self, selection: SdkSyntaxSelection, kind: NodeKind) -> SdkSyntaxSelection {
        let nodes = selection
            .nodes
            .into_iter()
            .filter(|id| self.snapshot.kind_of(id.node_id) == Some(kind))
            .collect();
        SdkSyntaxSelection {
            nodes,
            bounds: selection.bounds,
        }
    }

    pub fn pipeline(&self, root: SdkNodeRef) -> SdkSyntaxPipeline<'a> {
        SdkSyntaxPipeline::new(self.snapshot, root, self.bounds())
    }
}

impl<'a> SdkSyntaxPipeline<'a> {
    pub fn new(snapshot: &'a SyntaxSnapshot<'a>, root: SdkNodeRef, bounds: QueryBounds) -> Self {
        Self {
            snapshot,
            root,
            bounds,
            ops: Vec::new(),
        }
    }

    pub fn replace(mut self, target: SdkNodeRef, replacement: SdkNodeRef) -> Self {
        self.ops.push(PipelineOp {
            kind: PipelineOpKind::Replace,
            target,
            payload: Some(replacement),
        });
        self
    }

    pub fn remove(mut self, target: SdkNodeRef) -> Self {
        self.ops.push(PipelineOp {
            kind: PipelineOpKind::Remove,
            target,
            payload: None,
        });
        self
    }

    pub fn insert_before(mut self, anchor: SdkNodeRef, node: SdkNodeRef) -> Self {
        self.ops.push(PipelineOp {
            kind: PipelineOpKind::InsertBefore,
            target: anchor,
            payload: Some(node),
        });
        self
    }

    pub fn insert_after(mut self, anchor: SdkNodeRef, node: SdkNodeRef) -> Self {
        self.ops.push(PipelineOp {
            kind: PipelineOpKind::InsertAfter,
            target: anchor,
            payload: Some(node),
        });
        self
    }

    pub fn ordered_ops(&self) -> Vec<PipelineOp> {
        let mut ordered = self.ops.clone();
        ordered.sort_by_key(|op| {
            let precedence = match op.kind {
                PipelineOpKind::Remove => 0u8,
                PipelineOpKind::Replace => 1u8,
                PipelineOpKind::InsertBefore => 2u8,
                PipelineOpKind::InsertAfter => 3u8,
            };
            (op.target.node_id, precedence)
        });
        ordered
    }

    pub fn validate(&self) -> Result<(), PipelineValidationError> {
        let expected_gen = self.snapshot.generation_id();
        if self.root.syntax_generation_id != expected_gen {
            return Err(PipelineValidationError::StaleGeneration {
                expected: expected_gen,
                actual: self.root.syntax_generation_id,
            });
        }
        let mut seen_targets = std::collections::HashSet::new();
        for op in &self.ops {
            if op.target.syntax_generation_id != expected_gen {
                return Err(PipelineValidationError::StaleGeneration {
                    expected: expected_gen,
                    actual: op.target.syntax_generation_id,
                });
            }
            if self.snapshot.node_at(op.target.node_id).is_none() {
                return Err(PipelineValidationError::MissingNode { node: op.target });
            }
            if let Some(payload) = op.payload {
                if payload.syntax_generation_id != expected_gen {
                    return Err(PipelineValidationError::StaleGeneration {
                        expected: expected_gen,
                        actual: payload.syntax_generation_id,
                    });
                }
                if self.snapshot.node_at(payload.node_id).is_none() {
                    return Err(PipelineValidationError::MissingNode { node: payload });
                }
            }
            if matches!(op.kind, PipelineOpKind::Replace | PipelineOpKind::Remove)
                && !seen_targets.insert(op.target)
            {
                return Err(PipelineValidationError::Conflict { target: op.target });
            }
        }
        Ok(())
    }

    /// Temporary host behavior: validate and return the same root.
    /// Full structural rewrite application is performed by native runtime shims.
    pub fn apply(self) -> Result<SdkNodeRef, PipelineValidationError> {
        self.validate()?;
        Ok(self.root)
    }
}

/// Materialize a snapshot for the current syntax generation.
pub fn materialize_snapshot<'a>(
    program: &'a Spanned<Program>,
    syntax_generation_id: u64,
) -> SyntaxSnapshot<'a> {
    SyntaxSnapshot::from_program(program, syntax_generation_id)
}

/// Typed downcast helper for host-side `As*` lowering.
pub fn downcast_node<'a, T: crate::syntax_query::AstNode + 'static>(
    snapshot: &'a SyntaxSnapshot<'a>,
    id: SdkNodeRef,
) -> Option<&'a T> {
    let node = snapshot.resolve(id.to_stable())?;
    node.of::<T>()
}

/// Fluent Rust-side query entry (used by tests and future native shims).
pub fn query_at<'a>(snapshot: &'a SyntaxSnapshot<'a>, root: SdkNodeRef) -> Query<'a> {
    let node = snapshot
        .resolve(root.to_stable())
        .expect("query root must exist in snapshot");
    Query::from(node)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::parse_program;

    #[test]
    fn sdk_query_finds_function_and_parent() {
        let src = "pub fn f() { }";
        let program = parse_program(src).expect("parse");
        let snap = materialize_snapshot(&program, 1);
        let func_id = (0..snap.len() as u32)
            .find(|id| snap.kind_of(*id) == Some(NodeKind::FunctionDefinition))
            .expect("function indexed in snapshot");
        let func_ref = SdkNodeRef {
            syntax_generation_id: 1,
            node_id: func_id,
        };
        let q = SdkSyntaxQuery::at_program(&snap, &program);
        assert!(q.parent(func_ref).is_some());
        let typed = query_at(&snap, func_ref).find_first::<crate::syntax::FunctionDefinition>();
        assert!(typed.is_some());
    }

    #[test]
    fn sdk_query_resolves_span_for_function() {
        let src = "pub fn f() { let x = 1; }";
        let program = parse_program(src).expect("parse");
        let snap = materialize_snapshot(&program, 7);
        let mut q = SdkSyntaxQuery::at_program(&snap, &program);
        let func = q
            .find_first_typed::<crate::syntax::FunctionDefinition>()
            .expect("function");
        let span = q.span(func).expect("function span");
        assert!(span.start < span.end);
        assert!(span.line_start > 0);
    }

    #[test]
    fn syntax_pipeline_detects_conflicts_deterministically() {
        let src = "pub fn f() { }";
        let program = parse_program(src).expect("parse");
        let snap = materialize_snapshot(&program, 1);
        let mut q = SdkSyntaxQuery::at_program(&snap, &program);
        let func = q
            .find_first_typed::<crate::syntax::FunctionDefinition>()
            .expect("function");
        let pipeline = q.pipeline(func).replace(func, func).remove(func);
        let ordered = pipeline.ordered_ops();
        assert_eq!(ordered.len(), 2);
        assert!(matches!(
            pipeline.validate(),
            Err(PipelineValidationError::Conflict { .. })
        ));
    }
}
