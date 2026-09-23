use std::collections::HashMap;
use std::path::PathBuf;

use crate::resolve::{ItemId, Resolution, ResolvedValue, canonical_item_id};
use crate::syntax::{AstNodeId, SpanInfo, Spanned};
use crate::syntax::{Expression, Program};
use crate::types::path_value::PathTypeEnv;
use crate::types::result::CallLoweringKind;
use crate::types::{TypeId, TypeInfo, TypeTable};

use super::super::compatibility::{is_never, is_numeric};
use super::super::model::{CastIntent, LoweringPrep, LoweringPrepSurfaces};
use crate::types::path_value::named_item_id;

impl LoweringPrep {
    /// Walk typed syntax and populate call kinds and cast intents (no type inference).
    pub fn run(
        program: &Spanned<Program>,
        resolution: &Resolution,
        node_types: &HashMap<AstNodeId, TypeId>,
        surfaces: &LoweringPrepSurfaces<'_>,
    ) -> Self {
        let mut walker = PrepWalker::new(resolution, node_types, surfaces);
        for item in &program.node.items {
            walker.walk_item(item);
        }
        walker.finish()
    }
}

pub(super) struct PrepWalker<'a> {
    pub(super) resolution: &'a Resolution,
    pub(super) node_types: &'a HashMap<AstNodeId, TypeId>,
    pub(super) surfaces: &'a LoweringPrepSurfaces<'a>,
    pub(super) prep: LoweringPrep,
    pub(super) current_source_path: Option<PathBuf>,
    pub(super) current_return_type: Option<TypeId>,
    pub(super) contextual_expected_type: Option<TypeId>,
    pub(super) generic_params: HashMap<String, TypeId>,
}

impl<'a> PrepWalker<'a> {
    pub(super) fn new(
        resolution: &'a Resolution,
        node_types: &'a HashMap<AstNodeId, TypeId>,
        surfaces: &'a LoweringPrepSurfaces<'a>,
    ) -> Self {
        Self {
            resolution,
            node_types,
            surfaces,
            prep: LoweringPrep::default(),
            current_source_path: None,
            current_return_type: None,
            contextual_expected_type: None,
            generic_params: HashMap::new(),
        }
    }

    pub(super) fn finish(mut self) -> LoweringPrep {
        self.prep.cast_intents.sort_by_key(|intent| {
            (
                intent.source_path.as_ref().map(|path| path.to_string_lossy().into_owned()).unwrap_or_default(),
                intent.node_id.0,
                intent.span.start,
                intent.span.end,
                intent.from.0,
                intent.to.0,
            )
        });
        self.prep.cast_intents.dedup_by(|left, right| {
            left.node_id == right.node_id
                && left.source_path == right.source_path
                && left.span == right.span
                && left.from == right.from
                && left.to == right.to
        });
        self.prep
    }

    pub(super) fn node_type(&self, id: AstNodeId) -> Option<TypeId> {
        self.node_types.get(&id).copied()
    }

    pub(super) fn expr_type(&self, expr: &Spanned<Expression>) -> Option<TypeId> {
        self.node_type(expr.id)
    }

    pub(super) fn record_call_kind(&mut self, node_id: AstNodeId, kind: CallLoweringKind) {
        self.prep.call_kinds.insert(node_id, kind);
    }

    pub(super) fn record_numeric_cast(&mut self, node_id: AstNodeId, span: SpanInfo, expected: TypeId, actual: TypeId) {
        if types_compatible_without_cast(self.surfaces.types, self.resolution, expected, actual) {
            return;
        }
        if !is_numeric(self.surfaces.types, expected) || !is_numeric(self.surfaces.types, actual) {
            return;
        }
        if self
            .prep
            .cast_intents
            .iter()
            .any(|intent| intent.node_id == node_id && intent.from == actual && intent.to == expected)
        {
            return;
        }
        if self
            .prep
            .cast_intents
            .iter()
            .any(|intent| intent.node_id == node_id && intent.from == expected && intent.to == actual)
        {
            return;
        }
        self.prep.cast_intents.push(CastIntent {
            node_id,
            span,
            from: actual,
            to: expected,
            source_path: self.current_source_path.clone(),
        });
    }

    pub(super) fn resolved_value_at(&self, span: SpanInfo) -> Option<ResolvedValue> {
        let value = self.resolution.tables.resolved_value_at(span, self.current_source_path.as_ref())?;
        Some(match value {
            ResolvedValue::Item(item_id) => ResolvedValue::Item(canonical_item_id(self.resolution, item_id)),
            other => other,
        })
    }

    pub(super) fn item_id_for_span(&self, span: SpanInfo) -> Option<ItemId> {
        if let Some(path) = &self.current_source_path
            && let Some(info) = self.resolution.items.iter().find(|info| {
                info.span == span && info.source_path.as_ref().is_some_and(|source| crate::paths::same_file(source, path))
            })
        {
            return Some(info.id);
        }
        match self.resolution.items.iter().filter(|info| info.span == span).collect::<Vec<_>>().as_slice() {
            [single] => Some(single.id),
            _ => None,
        }
    }

    pub(super) fn return_type_for_item_span(&self, span: SpanInfo) -> Option<TypeId> {
        let item_id = self.item_id_for_span(span)?;
        self.surfaces
            .function_signatures
            .get(&item_id)
            .map(|s| s.return_type)
            .or_else(|| self.surfaces.method_function_signatures.get(&item_id).map(|s| s.return_type))
    }
}

fn types_compatible_without_cast(types: &TypeTable, resolution: &Resolution, expected: TypeId, actual: TypeId) -> bool {
    if expected == actual || is_never(types, expected) || is_never(types, actual) {
        return true;
    }
    if let (Some(TypeInfo::Primitive(a)), Some(TypeInfo::Primitive(b))) = (types.get(expected), types.get(actual))
        && a == b
    {
        return true;
    }
    if let (Some(TypeInfo::Array(a)), Some(TypeInfo::Array(b))) = (types.get(expected), types.get(actual))
        && a == b
    {
        return true;
    }
    if let (Some(TypeInfo::Fiber(a)), Some(TypeInfo::Fiber(b))) = (types.get(expected), types.get(actual))
        && a == b
    {
        return true;
    }
    if let Some(TypeInfo::Fiber(p)) = types.get(actual)
        && let Some(TypeInfo::Applied { base, args }) = types.get(expected)
        && args.len() == 1
        && args[0] == *p
        && resolution.items.get(base.0).is_some_and(|i| i.name == "Fiber" || i.name.ends_with("::Fiber"))
    {
        return true;
    }
    let env = PathTypeEnv {
        types,
        local_types: &HashMap::new(),
        struct_fields_ordered: &HashMap::new(),
        generic_items: &HashMap::new(),
    };
    named_item_id(&env, expected).is_some() && named_item_id(&env, expected) == named_item_id(&env, actual)
}
