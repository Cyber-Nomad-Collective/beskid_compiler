//! Codegen metadata produced after type checking: call dispatch kinds and numeric cast intents.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::hir::{
    HirBlock, HirCallExpression, HirElseBranch, HirExpressionNode, HirIfStatement, HirItem, HirMatchExpression,
    HirMethodDefinition, HirPattern, HirPrimitiveType, HirProgram, HirStatementNode, HirStructLiteralExpression,
    HirType,
};
use crate::paths;
use crate::resolve::{HirNodeId, ItemId, LocalId, Resolution, ResolvedType, ResolvedValue, canonical_item_id};
use crate::syntax::{SpanInfo, Spanned};
use crate::types::path_value::{
    PathTypeEnv, field_type_on_receiver, first_field_segment_name, generic_mapping_for_type_id,
    method_name_from_path_callee, named_item_id, receiver_type_for_path_callee, resolve_path_base_local,
    struct_fields_for_item,
};
use crate::types::result::{CallLoweringKind, FunctionSignature, MethodReceiverSource};
use crate::types::{TypeId, TypeInfo, TypeTable};
/// Cast intent keyed by HIR node id (span retained for diagnostics).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CastIntent {
    pub node_id: HirNodeId,
    pub span: SpanInfo,
    pub from: TypeId,
    pub to: TypeId,
    pub source_path: Option<PathBuf>,
}

/// Call dispatch and cast metadata for codegen lowering.
#[derive(Debug, Default, Clone)]
pub struct LoweringPrep {
    pub call_kinds: HashMap<HirNodeId, CallLoweringKind>,
    pub cast_intents: Vec<CastIntent>,
}

/// Read-only type surface inputs for lowering prep (orchestrator merges unit surfaces).
pub struct LoweringPrepSurfaces<'a> {
    pub types: &'a TypeTable,
    pub local_types: &'a HashMap<LocalId, TypeId>,
    pub function_signatures: &'a HashMap<ItemId, FunctionSignature>,
    pub method_function_signatures: &'a HashMap<ItemId, FunctionSignature>,
    pub struct_fields_ordered: &'a HashMap<ItemId, Vec<(String, TypeId)>>,
    pub struct_event_fields: &'a HashMap<ItemId, HashMap<String, Option<usize>>>,
    pub enum_variants_ordered: &'a HashMap<ItemId, Vec<(String, Vec<TypeId>)>>,
    pub generic_items: &'a HashMap<ItemId, Vec<String>>,
    pub methods_by_receiver: &'a HashMap<(ItemId, String), ItemId>,
    pub contract_signatures: &'a HashMap<(ItemId, String), FunctionSignature>,
    pub named_types: &'a HashMap<ItemId, TypeId>,
}

impl<'a> LoweringPrepSurfaces<'a> {
    pub fn path_env(&self) -> PathTypeEnv<'a> {
        PathTypeEnv {
            types: self.types,
            local_types: self.local_types,
            struct_fields_ordered: self.struct_fields_ordered,
            generic_items: self.generic_items,
        }
    }
}

impl LoweringPrep {
    pub fn call_kind_at(&self, node_id: HirNodeId) -> Option<&CallLoweringKind> {
        self.call_kinds.get(&node_id)
    }

    pub fn cast_intents_for_node(&self, node_id: HirNodeId) -> impl Iterator<Item = &CastIntent> {
        self.cast_intents.iter().filter(move |intent| intent.node_id == node_id)
    }

    /// Walk typed HIR and populate call kinds and cast intents (no type inference).
    pub fn run(
        program: &Spanned<HirProgram>,
        resolution: &Resolution,
        node_types: &HashMap<HirNodeId, TypeId>,
        surfaces: &LoweringPrepSurfaces<'_>,
    ) -> Self {
        let mut walker = PrepWalker::new(resolution, node_types, surfaces);
        for item in &program.node.items {
            walker.walk_item(item);
        }
        walker.finish()
    }
}

struct PrepWalker<'a> {
    resolution: &'a Resolution,
    node_types: &'a HashMap<HirNodeId, TypeId>,
    surfaces: &'a LoweringPrepSurfaces<'a>,
    prep: LoweringPrep,
    current_source_path: Option<PathBuf>,
    current_return_type: Option<TypeId>,
    contextual_expected_type: Option<TypeId>,
    generic_params: HashMap<String, TypeId>,
}

impl<'a> PrepWalker<'a> {
    fn new(
        resolution: &'a Resolution,
        node_types: &'a HashMap<HirNodeId, TypeId>,
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

    fn finish(mut self) -> LoweringPrep {
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

    fn node_type(&self, id: HirNodeId) -> Option<TypeId> {
        self.node_types.get(&id).copied()
    }

    fn expr_type(&self, expr: &Spanned<HirExpressionNode>) -> Option<TypeId> {
        self.node_type(expr.id)
    }

    fn record_call_kind(&mut self, node_id: HirNodeId, kind: CallLoweringKind) {
        self.prep.call_kinds.insert(node_id, kind);
    }

    fn record_numeric_cast(&mut self, node_id: HirNodeId, span: SpanInfo, expected: TypeId, actual: TypeId) {
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

    fn resolved_value_at(&self, span: SpanInfo) -> Option<ResolvedValue> {
        let value = self.resolution.tables.resolved_value_at(span, self.current_source_path.as_ref())?;
        Some(match value {
            ResolvedValue::Item(item_id) => ResolvedValue::Item(canonical_item_id(self.resolution, item_id)),
            other => other,
        })
    }

    fn item_id_for_span(&self, span: SpanInfo) -> Option<ItemId> {
        if let Some(path) = &self.current_source_path
            && let Some(info) = self.resolution.items.iter().find(|info| {
                info.span == span && info.source_path.as_ref().is_some_and(|source| paths::same_file(source, path))
            })
        {
            return Some(info.id);
        }
        match self.resolution.items.iter().filter(|info| info.span == span).collect::<Vec<_>>().as_slice() {
            [single] => Some(single.id),
            _ => None,
        }
    }

    fn return_type_for_item_span(&self, span: SpanInfo) -> Option<TypeId> {
        let item_id = self.item_id_for_span(span)?;
        self.surfaces
            .function_signatures
            .get(&item_id)
            .map(|s| s.return_type)
            .or_else(|| self.surfaces.method_function_signatures.get(&item_id).map(|s| s.return_type))
    }

    fn method_item_for_receiver(&self, receiver_type: TypeId, method_name: &str) -> Option<ItemId> {
        let receiver_item = named_item_id(&self.surfaces.path_env(), receiver_type)?;
        self.surfaces
            .methods_by_receiver
            .get(&(receiver_item, method_name.to_string()))
            .copied()
            .map(|item| canonical_item_id(self.resolution, item))
    }

    fn method_dispatch_signature(&self, method_item_id: ItemId, receiver_type: TypeId) -> Option<FunctionSignature> {
        let signature = self
            .surfaces
            .method_function_signatures
            .get(&method_item_id)
            .or_else(|| self.surfaces.function_signatures.get(&method_item_id))?
            .clone();
        let mapping = generic_mapping_for_type_id(&self.surfaces.path_env(), receiver_type);
        if mapping.is_empty() {
            return Some(signature);
        }
        Some(FunctionSignature {
            params: signature.params.iter().map(|p| substitute_type_id(self.surfaces, *p, &mapping)).collect(),
            return_type: substitute_type_id(self.surfaces, signature.return_type, &mapping),
        })
    }

    fn named_type_id(&self, item_id: ItemId) -> Option<TypeId> {
        self.surfaces.named_types.get(&item_id).copied().or_else(|| find_named_type(self.surfaces.types, item_id))
    }

    fn type_id_for_hir_type(&self, ty: &Spanned<HirType>) -> Option<TypeId> {
        match &ty.node {
            HirType::Primitive(p) => primitive_type_id(self.surfaces.types, p.node),
            HirType::Complex(path) => {
                if path.node.segments.len() == 1
                    && path.node.segments[0].node.type_args.is_empty()
                    && let Some(id) = self.generic_params.get(&path.node.segments[0].node.name.node.name)
                {
                    return Some(*id);
                }
                self.type_id_for_type_path(path)
            }
            HirType::Array(inner) => {
                let inner_id = self.type_id_for_hir_type(inner)?;
                self.surfaces.types.find_array_of(inner_id)
            }
            HirType::Function { return_type, parameters } => {
                let ret = self.type_id_for_hir_type(return_type)?;
                let params = parameters.iter().map(|p| self.type_id_for_hir_type(p)).collect::<Option<Vec<_>>>()?;
                lookup_function_type(self.surfaces.types, &params, ret)
            }
        }
    }

    fn type_id_for_type_path(&self, path: &Spanned<crate::hir::HirPath>) -> Option<TypeId> {
        let ResolvedType::Item(item_id) =
            self.resolution.tables.resolved_type_at(path.span, self.current_source_path.as_ref())?
        else {
            return None;
        };
        let item_id = canonical_item_id(self.resolution, item_id);
        let base = self.named_type_id(item_id)?;
        let last = path.node.segments.last()?;
        if last.node.type_args.is_empty() {
            return Some(base);
        }
        let args = last.node.type_args.iter().map(|a| self.type_id_for_hir_type(a)).collect::<Option<Vec<_>>>()?;
        find_applied_type(self.surfaces.types, item_id, &args)
    }

    fn walk_item(&mut self, item: &Spanned<HirItem>) {
        match &item.node {
            HirItem::FunctionDefinition(def) => {
                self.with_source_path_from_item(item.span, |w| {
                    let mut generics = Vec::new();
                    for g in &def.node.generics {
                        if let Some(id) = find_generic_param(w.surfaces.types, &g.node.name) {
                            w.generic_params.insert(g.node.name.clone(), id);
                            generics.push(g.node.name.clone());
                        }
                    }
                    w.current_return_type = def
                        .node
                        .return_type
                        .as_ref()
                        .and_then(|t| w.type_id_for_hir_type(t))
                        .or_else(|| w.return_type_for_item_span(item.span))
                        .or_else(|| primitive_type_id(w.surfaces.types, HirPrimitiveType::Unit));
                    w.walk_block(&def.node.body);
                    for name in generics {
                        w.generic_params.remove(&name);
                    }
                });
            }
            HirItem::MethodDefinition(def) => self.walk_method_definition(item.span, def),
            HirItem::ExtendTypeDefinition(def) => {
                for m in &def.node.methods {
                    self.walk_method_definition(m.span, m);
                }
            }
            HirItem::TestDefinition(def) => self.with_source_path_from_item(item.span, |w| {
                w.current_return_type = primitive_type_id(w.surfaces.types, HirPrimitiveType::Unit);
                w.walk_block(&def.node.body);
            }),
            HirItem::TypeDefinition(def) => {
                for m in &def.node.methods {
                    self.walk_method_definition(m.span, m);
                }
            }
            HirItem::InlineModule(m) => {
                for nested in &m.node.items {
                    self.walk_item(nested);
                }
            }
            _ => {}
        }
    }

    fn walk_method_definition(&mut self, span: SpanInfo, def: &Spanned<HirMethodDefinition>) {
        self.with_source_path_from_item(span, |w| {
            w.current_return_type = def
                .node
                .return_type
                .as_ref()
                .and_then(|t| w.type_id_for_hir_type(t))
                .or_else(|| w.return_type_for_item_span(span))
                .or_else(|| primitive_type_id(w.surfaces.types, HirPrimitiveType::Unit));
            w.walk_block(&def.node.body);
        });
    }

    fn with_source_path_from_item(&mut self, span: SpanInfo, f: impl FnOnce(&mut Self)) {
        let prev = self.current_source_path.clone();
        if let Some(info) = self.resolution.items.iter().find(|i| i.span == span) {
            self.current_source_path = info.source_path.as_ref().map(|p| paths::unit_path_key(p.as_path()));
        }
        f(self);
        self.current_source_path = prev;
    }

    fn walk_block(&mut self, block: &Spanned<HirBlock>) {
        for stmt in &block.node.statements {
            self.walk_statement(stmt);
        }
    }

    fn walk_statement(&mut self, stmt: &Spanned<HirStatementNode>) {
        match &stmt.node {
            HirStatementNode::LetStatement(let_stmt) => {
                if let Some(ty) = &let_stmt.node.type_annotation {
                    let expected = self.type_id_for_hir_type(ty);
                    let prev = self.contextual_expected_type;
                    if let Some(e) = expected {
                        self.contextual_expected_type = Some(e);
                    }
                    self.walk_expression(&let_stmt.node.value);
                    self.contextual_expected_type = prev;
                    if let (Some(e), Some(a)) = (expected, self.expr_type(&let_stmt.node.value)) {
                        self.record_numeric_cast(let_stmt.node.value.id, let_stmt.node.name.span, e, a);
                    }
                } else {
                    self.walk_expression(&let_stmt.node.value);
                }
            }
            HirStatementNode::ReturnStatement(ret) => {
                let prev = self.contextual_expected_type;
                if let Some(e) = self.current_return_type {
                    self.contextual_expected_type = Some(e);
                }
                if let Some(expr) = &ret.node.value {
                    self.walk_expression(expr);
                    if let (Some(e), Some(a)) = (self.current_return_type, self.expr_type(expr)) {
                        self.record_numeric_cast(ret.id, ret.span, e, a);
                    }
                }
                self.contextual_expected_type = prev;
            }
            HirStatementNode::WhileStatement(w) => {
                self.walk_expression(&w.node.condition);
                self.walk_block(&w.node.body);
            }
            HirStatementNode::ForStatement(f) => {
                self.walk_expression(&f.node.iterable);
                self.walk_block(&f.node.body);
            }
            HirStatementNode::IfStatement(i) => self.walk_if(i),
            HirStatementNode::ExpressionStatement(e) => self.walk_expression(&e.node.expression),
            _ => {}
        }
    }

    fn walk_if(&mut self, if_stmt: &Spanned<HirIfStatement>) {
        self.walk_expression(&if_stmt.node.condition);
        self.walk_block(&if_stmt.node.then_block);
        if let Some(e) = &if_stmt.node.else_branch {
            match &e.node {
                HirElseBranch::Block(b) => self.walk_block(b),
                HirElseBranch::If(n) => self.walk_if(n),
            }
        }
    }

    fn walk_expression(&mut self, expr: &Spanned<HirExpressionNode>) {
        match &expr.node {
            HirExpressionNode::CallExpression(call) => {
                self.prep_call(expr.id, call);
                self.walk_expression(&call.node.callee);
                for a in &call.node.args {
                    self.walk_expression(a);
                }
            }
            HirExpressionNode::AssignExpression(a) => {
                self.walk_expression(&a.node.target);
                self.walk_expression(&a.node.value);
                if let (Some(t), Some(v)) = (self.expr_type(&a.node.target), self.expr_type(&a.node.value)) {
                    self.record_numeric_cast(expr.id, expr.span, t, v);
                }
            }
            HirExpressionNode::LambdaExpression(l) => {
                let sig = self.contextual_expected_type.and_then(|id| match self.surfaces.types.get(id)? {
                    TypeInfo::Function { params, return_type } => Some((params.clone(), *return_type)),
                    _ => None,
                });
                self.walk_expression(&l.node.body);
                if let (Some((_, er)), Some(rt)) = (sig.as_ref(), self.expr_type(&l.node.body)) {
                    self.record_numeric_cast(l.node.body.id, l.node.body.span, *er, rt);
                }
            }
            HirExpressionNode::StructLiteralExpression(lit) => {
                self.prep_struct_literal_casts(expr.id, lit);
                for f in &lit.node.fields {
                    self.walk_expression(&f.node.value);
                }
            }
            HirExpressionNode::EnumConstructorExpression(c) => {
                self.prep_enum_ctor_casts(c);
                for a in &c.node.args {
                    self.walk_expression(a);
                }
            }
            HirExpressionNode::MatchExpression(m) => self.prep_match(m),
            HirExpressionNode::BinaryExpression(b) => {
                self.walk_expression(&b.node.left);
                self.walk_expression(&b.node.right);
            }
            HirExpressionNode::UnaryExpression(u) => self.walk_expression(&u.node.expr),
            HirExpressionNode::GroupedExpression(g) => self.walk_expression(&g.node.expr),
            HirExpressionNode::BlockExpression(b) => self.walk_block(&b.node.block),
            HirExpressionNode::MemberExpression(m) => self.walk_expression(&m.node.target),
            HirExpressionNode::IndexExpression(i) => {
                self.walk_expression(&i.node.target);
                self.walk_expression(&i.node.index);
            }
            HirExpressionNode::ArrayLiteralExpression(a) => {
                for e in &a.node.elements {
                    self.walk_expression(e);
                }
            }
            HirExpressionNode::TryExpression(t) => self.walk_expression(&t.node.expr),
            HirExpressionNode::SpawnExpression(s) => self.walk_expression(&s.node.callee),
            _ => {}
        }
    }

    fn prep_struct_literal_casts(&mut self, expr_id: HirNodeId, lit: &Spanned<HirStructLiteralExpression>) {
        let Some(type_id) = self.node_type(expr_id).or_else(|| self.type_id_for_type_path(&lit.node.path)) else {
            return;
        };
        let Some(item_id) = named_item_id(&self.surfaces.path_env(), type_id) else {
            return;
        };
        let mapping = generic_mapping_for_type_id(&self.surfaces.path_env(), type_id);
        let path_env = self.surfaces.path_env();
        let Some(fields) =
            struct_fields_for_item(&path_env, self.resolution, item_id, self.current_source_path.as_ref())
        else {
            return;
        };
        for field in &lit.node.fields {
            let name = field.node.name.node.name.as_str();
            let Some((_, expected)) = fields.iter().find(|(n, _)| n.as_str() == name) else {
                continue;
            };
            let expected =
                if mapping.is_empty() { *expected } else { substitute_type_id(self.surfaces, *expected, &mapping) };
            if let Some(actual) = self.expr_type(&field.node.value) {
                self.record_numeric_cast(field.node.value.id, field.node.value.span, expected, actual);
            }
        }
    }

    fn prep_enum_ctor_casts(&mut self, ctor: &Spanned<crate::hir::HirEnumConstructorExpression>) {
        let Some(type_id) =
            self.resolution.tables.resolved_type_at(ctor.node.path.span, self.current_source_path.as_ref()).and_then(
                |r| match r {
                    ResolvedType::Item(id) => self.named_type_id(canonical_item_id(self.resolution, id)),
                    _ => None,
                },
            )
        else {
            return;
        };
        let Some(item_id) = named_item_id(&self.surfaces.path_env(), type_id) else {
            return;
        };
        let variant = ctor.node.path.node.variant.node.name.as_str();
        let mapping = generic_mapping_for_type_id(&self.surfaces.path_env(), type_id);
        let Some(fields) = self
            .surfaces
            .enum_variants_ordered
            .get(&item_id)
            .and_then(|vars| vars.iter().find(|(n, _)| n == variant))
            .map(|(_, fs)| {
                if mapping.is_empty() {
                    fs.clone()
                } else {
                    fs.iter().map(|f| substitute_type_id(self.surfaces, *f, &mapping)).collect()
                }
            })
        else {
            return;
        };
        for (arg, expected) in ctor.node.args.iter().zip(fields.iter()) {
            if let Some(actual) = self.expr_type(arg) {
                self.record_numeric_cast(arg.id, arg.span, *expected, actual);
            }
        }
    }

    fn prep_match(&mut self, m: &Spanned<HirMatchExpression>) {
        let scrutinee = self.expr_type(&m.node.scrutinee);
        self.walk_expression(&m.node.scrutinee);
        let mut expected = self.contextual_expected_type;
        for arm in &m.node.arms {
            if let Some(g) = &arm.node.guard {
                self.walk_expression(g);
            }
            self.prep_pattern_casts(scrutinee, &arm.node.pattern);
            let prev = self.contextual_expected_type;
            self.contextual_expected_type = expected;
            self.walk_expression(&arm.node.value);
            self.contextual_expected_type = prev;
            if let Some(actual) = self.expr_type(&arm.node.value) {
                if let Some(e) = expected {
                    if !is_never(self.surfaces.types, e) && !is_never(self.surfaces.types, actual) {
                        self.record_numeric_cast(arm.node.value.id, arm.node.value.span, e, actual);
                    }
                } else {
                    expected = Some(actual);
                }
            }
        }
    }

    fn prep_pattern_casts(&mut self, scrutinee: Option<TypeId>, pattern: &Spanned<HirPattern>) {
        let Some(expected) = scrutinee else {
            return;
        };
        match &pattern.node {
            HirPattern::Literal(lit) => {
                if let Some(actual) = literal_type_id(self.surfaces.types, &lit.node) {
                    self.record_numeric_cast(pattern.id, pattern.span, expected, actual);
                }
            }
            HirPattern::Enum(ep) => {
                if let Some(actual) = self
                    .resolution
                    .tables
                    .resolved_type_at(ep.node.path.span, self.current_source_path.as_ref())
                    .and_then(|r| match r {
                        ResolvedType::Item(id) => self.named_type_id(canonical_item_id(self.resolution, id)),
                        _ => None,
                    })
                {
                    let ok = actual == expected
                        || named_item_id(&self.surfaces.path_env(), actual)
                            == named_item_id(&self.surfaces.path_env(), expected);
                    if !ok {
                        self.record_numeric_cast(pattern.id, pattern.span, expected, actual);
                    }
                }
                for p in &ep.node.items {
                    self.prep_pattern_casts(scrutinee, p);
                }
            }
            _ => {}
        }
    }

    fn prep_call(&mut self, call_id: HirNodeId, call: &Spanned<HirCallExpression>) {
        if let Some(kind) = self.event_call_kind(&call.node.callee) {
            self.record_call_kind(call_id, kind);
            return;
        }

        if let HirExpressionNode::PathExpression(path) = &call.node.callee.node {
            let segs = &path.node.path.node.segments;
            let src = self.current_source_path.as_ref();
            if segs.len() >= 2
                && let Some(method) = method_name_from_path_callee(segs)
                && let Some((local, recv)) = receiver_type_for_path_callee(
                    self.resolution,
                    &self.surfaces.path_env(),
                    path.node.path.span,
                    segs,
                    src,
                )
            {
                if let Some(mid) = self.method_item_for_receiver(recv, method)
                    && let Some(sig) = self.method_dispatch_signature(mid, recv)
                {
                    self.record_call_kind(
                        call_id,
                        CallLoweringKind::MethodDispatch {
                            method_item_id: mid,
                            receiver_source: MethodReceiverSource::Local(local),
                            receiver_type: recv,
                        },
                    );
                    self.prep_arg_casts(&call.node.args, &sig.params);
                    return;
                }
                if let Some(cid) = named_item_id(&self.surfaces.path_env(), recv)
                    && let Some(sig) = self.surfaces.contract_signatures.get(&(cid, method.to_string()))
                {
                    self.record_call_kind(
                        call_id,
                        CallLoweringKind::ContractDispatch {
                            contract_item_id: cid,
                            receiver_source: MethodReceiverSource::Local(local),
                            receiver_type: recv,
                        },
                    );
                    self.prep_arg_casts(&call.node.args, &sig.params);
                    return;
                }
            }
            if segs.len() >= 2
                && let Some(ResolvedValue::Item(cid)) = self.resolved_value_at(path.node.path.span)
                && let Some(method) = method_name_from_path_callee(segs)
                && let Some(sig) = self.surfaces.contract_signatures.get(&(cid, method.to_string()))
                && let Some(recv) = self.named_type_id(cid)
            {
                self.record_call_kind(
                    call_id,
                    CallLoweringKind::ContractDispatch {
                        contract_item_id: cid,
                        receiver_source: MethodReceiverSource::Expression(path.node.path.span),
                        receiver_type: recv,
                    },
                );
                self.prep_arg_casts(&call.node.args, &sig.params);
                return;
            }
        }

        if let HirExpressionNode::MemberExpression(mem) = &call.node.callee.node {
            if let HirExpressionNode::PathExpression(path) = &mem.node.target.node
                && let Some(ResolvedValue::Item(cid)) = self.resolved_value_at(path.node.path.span)
                && let Some(sig) =
                    self.surfaces.contract_signatures.get(&(cid, mem.node.member.node.name.as_str().to_string()))
                && let Some(recv) = self.named_type_id(cid)
            {
                self.record_call_kind(
                    call_id,
                    CallLoweringKind::ContractDispatch {
                        contract_item_id: cid,
                        receiver_source: MethodReceiverSource::Expression(mem.node.target.span),
                        receiver_type: recv,
                    },
                );
                self.prep_arg_casts(&call.node.args, &sig.params);
                return;
            }
            if let Some(recv) = self.expr_type(&mem.node.target) {
                let method = mem.node.member.node.name.as_str();
                if let Some(mid) = self.method_item_for_receiver(recv, method)
                    && let Some(sig) = self.method_dispatch_signature(mid, recv)
                {
                    self.record_call_kind(
                        call_id,
                        CallLoweringKind::MethodDispatch {
                            method_item_id: mid,
                            receiver_source: MethodReceiverSource::Expression(mem.node.target.span),
                            receiver_type: recv,
                        },
                    );
                    self.prep_arg_casts(&call.node.args, &sig.params);
                    return;
                }
                if let Some(cid) = named_item_id(&self.surfaces.path_env(), recv)
                    && let Some(sig) = self.surfaces.contract_signatures.get(&(cid, method.to_string()))
                {
                    self.record_call_kind(
                        call_id,
                        CallLoweringKind::ContractDispatch {
                            contract_item_id: cid,
                            receiver_source: MethodReceiverSource::Expression(mem.node.target.span),
                            receiver_type: recv,
                        },
                    );
                    self.prep_arg_casts(&call.node.args, &sig.params);
                    return;
                }
            }
        }

        let item_callee = matches!(&call.node.callee.node, HirExpressionNode::PathExpression(p)
            if matches!(self.resolved_value_at(p.node.path.span), Some(ResolvedValue::Item(_))));
        if !item_callee
            && let Some(ct) = self.expr_type(&call.node.callee)
            && let Some(TypeInfo::Function { params, .. }) = self.surfaces.types.get(ct)
        {
            self.record_call_kind(call_id, CallLoweringKind::CallableValueCall);
            self.prep_arg_casts(&call.node.args, params);
            return;
        }

        if let HirExpressionNode::PathExpression(p) = &call.node.callee.node
            && let Some(ResolvedValue::Item(id)) = self.resolved_value_at(p.node.path.span)
        {
            self.record_call_kind(call_id, CallLoweringKind::ItemCall { item_id: id });
            if let Some(sig) = self.surfaces.function_signatures.get(&id) {
                self.prep_arg_casts(&call.node.args, &sig.params);
            }
        }
    }

    fn prep_arg_casts(&mut self, args: &[Spanned<HirExpressionNode>], params: &[TypeId]) {
        for (arg, expected) in args.iter().zip(params.iter()) {
            if let Some(actual) = self.expr_type(arg) {
                self.record_numeric_cast(arg.id, arg.span, *expected, actual);
            }
        }
    }

    fn event_call_kind(&self, callee: &Spanned<HirExpressionNode>) -> Option<CallLoweringKind> {
        let (src, recv, item, field) = match &callee.node {
            HirExpressionNode::MemberExpression(m) => {
                let recv = self.expr_type(&m.node.target)?;
                let item = named_item_id(&self.surfaces.path_env(), recv)?;
                (
                    MethodReceiverSource::Expression(m.node.target.span),
                    recv,
                    item,
                    m.node.member.node.name.as_str().to_string(),
                )
            }
            HirExpressionNode::PathExpression(p) => {
                let segs = &p.node.path.node.segments;
                let field = first_field_segment_name(segs)?.to_string();
                let first = segs.first()?.node.name.node.name.as_str();
                let local = resolve_path_base_local(
                    self.resolution,
                    p.node.path.span,
                    first,
                    self.current_source_path.as_ref(),
                )?;
                let recv = self.surfaces.local_types.get(&local).copied()?;
                let item = named_item_id(&self.surfaces.path_env(), recv)?;
                (MethodReceiverSource::Local(local), recv, item, field)
            }
            _ => return None,
        };
        self.surfaces.struct_event_fields.get(&item).and_then(|f| f.get(&field))?;
        field_type_on_receiver(
            self.resolution,
            &self.surfaces.path_env(),
            recv,
            &field,
            self.current_source_path.as_ref(),
        )?;
        Some(CallLoweringKind::EventInvoke { receiver_source: src, receiver_type: recv })
    }
}

fn substitute_type_id(
    surfaces: &LoweringPrepSurfaces<'_>,
    type_id: TypeId,
    mapping: &HashMap<String, TypeId>,
) -> TypeId {
    match surfaces.types.get(type_id).cloned() {
        Some(TypeInfo::GenericParam(n)) => mapping.get(&n).copied().unwrap_or(type_id),
        Some(TypeInfo::Applied { base, args }) => {
            let new_args: Vec<TypeId> = args.iter().map(|a| substitute_type_id(surfaces, *a, mapping)).collect();
            if new_args == args {
                type_id
            } else {
                find_applied_type(surfaces.types, base, &new_args).unwrap_or(type_id)
            }
        }
        Some(TypeInfo::Array(el)) => {
            let sub = substitute_type_id(surfaces, el, mapping);
            if sub == el { type_id } else { surfaces.types.find_array_of(sub).unwrap_or(type_id) }
        }
        _ => type_id,
    }
}

fn primitive_type_id(types: &TypeTable, p: HirPrimitiveType) -> Option<TypeId> {
    types.find_primitive(p)
}

// NOTE: these scan the dense TypeId space and MUST stop at `types.len()`.
// An unbounded `(0..)` iterator never terminates when the target type was never
// interned, because `types.get(id)` returning `None` makes the `find` predicate
// `false` rather than ending iteration (this caused multi-hour CI hangs while
// resolving a named return type that was absent from the lowering surface).
fn lookup_function_type(types: &TypeTable, params: &[TypeId], ret: TypeId) -> Option<TypeId> {
    (0..types.len()).map(TypeId).find(|id| matches!(types.get(*id), Some(TypeInfo::Function { params: ps, return_type, }) if ps == params && *return_type == ret))
}

fn find_named_type(types: &TypeTable, item: ItemId) -> Option<TypeId> {
    (0..types.len()).map(TypeId).find(|id| matches!(types.get(*id), Some(TypeInfo::Named(i)) if *i == item))
}

fn find_generic_param(types: &TypeTable, name: &str) -> Option<TypeId> {
    (0..types.len()).map(TypeId).find(|id| matches!(types.get(*id), Some(TypeInfo::GenericParam(n)) if n == name))
}

fn find_applied_type(types: &TypeTable, base: ItemId, args: &[TypeId]) -> Option<TypeId> {
    (0..types.len())
        .map(TypeId)
        .find(|id| matches!(types.get(*id), Some(TypeInfo::Applied { base: b, args: a, }) if *b == base && a == args))
}

fn is_numeric(types: &TypeTable, id: TypeId) -> bool {
    matches!(
        types.get(id),
        Some(TypeInfo::Primitive(
            HirPrimitiveType::I32 | HirPrimitiveType::I64 | HirPrimitiveType::U8 | HirPrimitiveType::F64
        ))
    )
}

fn is_never(types: &TypeTable, id: TypeId) -> bool {
    matches!(types.get(id), Some(TypeInfo::Primitive(HirPrimitiveType::Never)))
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

fn literal_type_id(types: &TypeTable, lit: &crate::hir::HirLiteral) -> Option<TypeId> {
    use crate::hir::{HirLiteral, integer_literal_primitive_type};
    match lit {
        HirLiteral::Integer(v) => primitive_type_id(types, integer_literal_primitive_type(v)),
        HirLiteral::Float(_) => primitive_type_id(types, HirPrimitiveType::F64),
        HirLiteral::Bool(_) => primitive_type_id(types, HirPrimitiveType::Bool),
        HirLiteral::Char(_) => primitive_type_id(types, HirPrimitiveType::Char),
        HirLiteral::String(_) => primitive_type_id(types, HirPrimitiveType::String),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve::{HirNodeId, ModuleGraph};

    fn span(start: usize, end: usize) -> SpanInfo {
        SpanInfo { start, end, ..SpanInfo::default() }
    }

    fn table() -> TypeTable {
        let mut t = TypeTable::new();
        for p in [HirPrimitiveType::I32, HirPrimitiveType::I64, HirPrimitiveType::Unit, HirPrimitiveType::Never] {
            t.intern(TypeInfo::Primitive(p));
        }
        t
    }

    #[test]
    fn records_numeric_cast() {
        let types = table();
        let i32 = types.find_primitive(HirPrimitiveType::I32).unwrap();
        let i64 = types.find_primitive(HirPrimitiveType::I64).unwrap();
        let surfaces = LoweringPrepSurfaces {
            types: &types,
            local_types: &HashMap::new(),
            function_signatures: &HashMap::new(),
            method_function_signatures: &HashMap::new(),
            struct_fields_ordered: &HashMap::new(),
            struct_event_fields: &HashMap::new(),
            enum_variants_ordered: &HashMap::new(),
            generic_items: &HashMap::new(),
            methods_by_receiver: &HashMap::new(),
            contract_signatures: &HashMap::new(),
            named_types: &HashMap::new(),
        };
        let resolution = Resolution {
            items: Vec::new(),
            module_graph: ModuleGraph::default(),
            tables: crate::resolve::ResolutionTables::new(),
            span_index: Default::default(),
            warnings: Vec::new(),
            builtin_items: HashMap::new(),
            module_imports: HashMap::new(),
            symbols: crate::resolve::SymbolRegistry::default(),
            by_symbol: HashMap::new(),
        };
        let node_types = HashMap::new();
        let mut w = PrepWalker::new(&resolution, &node_types, &surfaces);
        w.record_numeric_cast(HirNodeId(1), span(0, 1), i64, i32);
        assert_eq!(w.prep.cast_intents.len(), 1);
    }
}
