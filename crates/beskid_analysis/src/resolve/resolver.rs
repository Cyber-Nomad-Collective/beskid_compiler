use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::hir::{HirProgram, HirItem};
use crate::syntax::Spanned;

use super::errors::ResolveResult;

use super::errors::{ResolveError, ResolveWarning};
use super::ids::{ItemId, ModuleId};
use super::items::ItemInfo;
use super::module_graph::ModuleGraph;
use super::symbol::{SymbolId, SymbolRegistry};
use super::tables::ResolutionTables;
use super::span_index::SpanIndex;

#[derive(Debug, Default)]
pub struct Resolver {
    pub(crate) items: Vec<ItemInfo>,
    pub(crate) module_graph: ModuleGraph,
    pub(crate) current_module: ModuleId,
    pub(crate) tables: ResolutionTables,
    pub(crate) local_scopes: Vec<HashMap<String, super::ids::LocalId>>,
    pub(crate) generic_scopes: Vec<HashMap<String, ()>>,
    pub(crate) errors: Vec<ResolveError>,
    pub(crate) warnings: Vec<ResolveWarning>,
    pub(crate) builtin_items: HashMap<ItemId, usize>,
    pub(crate) module_imports: HashMap<String, Vec<String>>,
    pub(crate) current_source_path: Option<PathBuf>,
    pub(crate) symbols: SymbolRegistry,
    pub(crate) by_symbol: HashMap<SymbolId, ItemId>,
    pub(crate) declaring_package: String,
    /// When resolving method bodies, the extended/receiver type for bare field access (`handle` → `this.handle`).
    pub(crate) current_receiver_item_id: Option<super::ids::ItemId>,
}

impl Resolver {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Optional correlation fields for [`resolve_program_traced`] / [`enter_resolve_span`].
#[derive(Debug, Clone, Copy, Default)]
pub struct ResolveTraceContext<'a> {
    pub entry_path: Option<&'a Path>,
    pub session_fingerprint: Option<&'a str>,
    pub syntax_generation_id: Option<u64>,
}

fn resolve_span(ctx: ResolveTraceContext<'_>) -> tracing::Span {
    tracing::info_span!(
        target: "beskid.analysis",
        "beskid.analysis.resolve",
        entry = tracing::field::display(
            ctx.entry_path
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<unknown>".to_string())
        ),
        session_fingerprint = tracing::field::display(
            ctx.session_fingerprint.unwrap_or("<none>")
        ),
        syntax_generation_id = ctx.syntax_generation_id.unwrap_or(0),
    )
}

/// Enters a `beskid.analysis.resolve` span nested under the active pipeline phase span.
pub fn enter_resolve_span(ctx: ResolveTraceContext<'_>) -> tracing::span::EnteredSpan {
    resolve_span(ctx).entered()
}

/// Resolve a program under a `beskid.analysis.resolve` tracing span.
pub fn resolve_program_traced(
    program: &Spanned<HirProgram>,
    ctx: ResolveTraceContext<'_>,
) -> ResolveResult<Resolution> {
    let _guard = enter_resolve_span(ctx);
    Resolver::new().resolve_program(program)
}

pub(super) fn path_segments(path: &Spanned<crate::hir::HirPath>) -> Vec<String> {
    path.node
        .segments
        .iter()
        .map(|segment| segment.node.name.node.name.clone())
        .collect()
}

pub(super) fn file_scoped_module_index(program: &Spanned<crate::hir::HirProgram>) -> Option<usize> {
    program.node.items.iter().position(|item| match &item.node {
        HirItem::ModuleDeclaration(def) => {
            def.node.visibility.node == crate::hir::HirVisibility::Private
                && def.node.attributes.is_empty()
        }
        _ => false,
    })
}

pub(super) fn file_scoped_module_path(
    program: &Spanned<crate::hir::HirProgram>,
) -> Option<Vec<String>> {
    let index = file_scoped_module_index(program)?;
    let HirItem::ModuleDeclaration(def) = &program.node.items.get(index)?.node else {
        return None;
    };
    Some(path_segments(&def.node.path))
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Resolution {
    pub items: Vec<ItemInfo>,
    pub module_graph: ModuleGraph,
    pub tables: ResolutionTables,
    pub span_index: SpanIndex,
    pub warnings: Vec<ResolveWarning>,
    pub builtin_items: HashMap<ItemId, usize>,
    pub module_imports: HashMap<String, Vec<String>>,
    pub symbols: SymbolRegistry,
    pub by_symbol: HashMap<SymbolId, ItemId>,
}

impl Resolution {
    pub fn rebuild_span_index(&mut self) {
        self.span_index = super::span_index::span_index_from_tables(&self.tables);
    }
}

