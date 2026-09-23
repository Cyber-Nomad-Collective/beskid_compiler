use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::syntax::Spanned;
use crate::syntax::{Node, Program, SpanInfo};

use super::errors::ResolveResult;

use super::errors::{ResolveError, ResolveWarning};
use super::ids::{ItemId, ModuleId};
use super::items::ItemInfo;
use super::module_graph::ModuleGraph;
use super::span_index::SpanIndex;
use super::symbol::{SymbolId, SymbolRegistry};
use super::tables::ResolutionTables;

#[derive(Debug, Default)]
pub struct Resolver {
    pub(crate) constants: HashMap<(ModuleId, String), crate::syntax::Spanned<crate::syntax::Literal>>,
    pub(crate) shared_constants: HashMap<String, Option<crate::syntax::Spanned<crate::syntax::Literal>>>,
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
    /// Successful public-item scope imports, keyed by the imported item name.
    pub(crate) imported_scope_origins: HashMap<String, SpanInfo>,
    /// Successful module-import aliases, keyed by the usable module alias.
    pub(crate) module_import_origins: HashMap<String, SpanInfo>,
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
pub fn resolve_program_traced(program: &Spanned<Program>, ctx: ResolveTraceContext<'_>) -> ResolveResult<Resolution> {
    let _guard = enter_resolve_span(ctx);
    Resolver::new().resolve_program(program)
}

pub(crate) fn path_segments(path: &Spanned<crate::syntax::Path>) -> Vec<String> {
    path.node.segments.iter().map(|segment| segment.node.name.node.name.clone()).collect()
}

/// Names recognized as primitive numeric conversion call forms (`i32(x)`, `byte(x)`, ...).
///
/// Kept in sync with `primitive_numeric_conversion_target` in
/// `crates/beskid_queries/src/semantic_contract/calls/facts.rs`, which classifies the same call
/// shape purely from AST text for typing/lowering. The two lists intentionally live in separate
/// crates (`beskid_queries` depends on `beskid_analysis`, not the reverse) and must be updated
/// together.
const PRIMITIVE_NUMERIC_CONVERSION_NAMES: &[&str] = &["i32", "i64", "u32", "u8", "byte", "word", "f64"];

/// True when `call` has the shape of a primitive numeric conversion call: an unqualified,
/// non-generic single-segment callee naming a conversion target, with exactly one argument.
/// Such calls are classified by AST shape alone (see [`PRIMITIVE_NUMERIC_CONVERSION_NAMES`]) and
/// never resolve to a declared item, so name resolution must not treat their callee as an
/// ordinary value reference.
pub(crate) fn is_primitive_numeric_conversion_call(call: &crate::syntax::CallExpression) -> bool {
    if call.args.len() != 1 {
        return false;
    }
    let crate::syntax::Expression::Path(path) = &call.callee.node else {
        return false;
    };
    let [segment] = path.node.path.node.segments.as_slice() else {
        return false;
    };
    if !segment.node.type_args.is_empty() {
        return false;
    }
    PRIMITIVE_NUMERIC_CONVERSION_NAMES.contains(&segment.node.name.node.name.as_str())
}

pub(super) fn file_scoped_module_index(program: &Spanned<crate::syntax::Program>) -> Option<usize> {
    program.node.items.iter().position(|item| match &item.node {
        Node::ModuleDeclaration(def) => {
            def.node.visibility.node == crate::syntax::Visibility::Private && def.node.attributes.is_empty()
        }
        _ => false,
    })
}

pub(super) fn file_scoped_module_path(program: &Spanned<crate::syntax::Program>) -> Option<Vec<String>> {
    let index = file_scoped_module_index(program)?;
    let Node::ModuleDeclaration(def) = &program.node.items.get(index)?.node else {
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
