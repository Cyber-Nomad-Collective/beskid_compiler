//! Syntax services consume the registered generation's owned index and tree.
use crate::{AstNodeKey, ModSemanticQueryAuthority, SourceUnitId};
use beskid_analysis::{
    mod_host::*,
    syntax::{AstNodeId, Node},
};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(crate) struct SyntaxState {
    invocations: HashMap<u64, Invocation>,
    closed: HashSet<u64>,
}
#[derive(Default)]
struct Invocation {
    issued: HashSet<AstNodeKey>,
    edits: Vec<Edit>,
    applied: Option<ModAppliedSyntax>,
}
struct Edit {
    index: usize,
    kind: EditKind,
    payload: Option<beskid_analysis::syntax::Spanned<Node>>,
    payload_doc: Option<beskid_analysis::doc::LeadingDocComment>,
}
#[derive(Clone, Copy)]
enum EditKind {
    Replace,
    Remove,
    Before,
    After,
}
const ITEMS: usize = 65536;
impl ModSemanticQueryAuthority<'_> {
    pub(crate) fn admit_semantic_invocation(&self, issuer: u64) -> Result<(), ModSemanticError> {
        let mut states = self.syntax_state.borrow_mut();
        if issuer == 0 || states.closed.contains(&issuer) {
            return Err(Self::error("catchall invocation is zero/closed"));
        }
        if states.invocations.len() + states.closed.len() >= 4096 && !states.invocations.contains_key(&issuer) {
            return Err(Self::error("semantic invocation budget exceeded"));
        }
        states.invocations.entry(issuer).or_default();
        Ok(())
    }
    pub(crate) fn syntax_key(&self, issuer: u64, node: &ModSyntaxNodeRef) -> Result<AstNodeKey, ModSemanticError> {
        if issuer == 0 || node.invocation_issuer != issuer || node.generation != self.assembly.generation {
            return Err(Self::error("foreign or expired syntax invocation/generation"));
        }
        let unit = self
            .registered_unit(&node.source_unit)
            .ok_or_else(|| Self::error("syntax unit is outside registered assembly"))?;
        let key = AstNodeKey {
            unit: SourceUnitId::new(self.db, unit.path.clone()),
            generation: node.generation,
            node: node.node,
        };
        let syntax = self.db.syntax_unit(key.unit).ok_or_else(|| Self::error("syntax unit unavailable"))?;
        if !syntax.accepts_key(self.db, key)
            || !self.syntax_state.borrow().invocations.get(&issuer).is_some_and(|state| state.issued.contains(&key))
        {
            return Err(Self::error("syntax reference was not issued by current invocation"));
        }
        Ok(key)
    }
    fn syntax_ref(&self, issuer: u64, key: AstNodeKey) -> Result<ModSyntaxNodeRef, ModSemanticError> {
        let mut states = self.syntax_state.borrow_mut();
        if states.closed.contains(&issuer) {
            return Err(Self::error("syntax invocation is closed"));
        }
        if states.invocations.len() + states.closed.len() >= 4096 && !states.invocations.contains_key(&issuer) {
            return Err(Self::error("syntax invocation budget exceeded"));
        }
        let state = states.invocations.entry(issuer).or_default();
        if state.issued.len() >= 1_000_000 && !state.issued.contains(&key) {
            return Err(Self::error("issued syntax node budget exceeded"));
        }
        state.issued.insert(key);
        Ok(ModSyntaxNodeRef {
            source_unit: key.unit.path(self.db).clone(),
            invocation_issuer: issuer,
            generation: key.generation,
            node: key.node,
        })
    }
    fn syntax_walk(
        &self,
        key: AstNodeKey,
        bounds: ModSyntaxBounds,
        ancestors: bool,
    ) -> Result<Vec<AstNodeKey>, ModSemanticError> {
        let bounds = bounds.checked()?;
        let syntax = self.db.syntax_unit(key.unit).ok_or_else(|| Self::error("syntax unit unavailable"))?;
        let index = syntax.syntax_index(self.db);
        let mut pending = if ancestors {
            index
                .metadata_for(key.generation, key.node)
                .and_then(|m| m.parent)
                .map(|p| vec![(p, 1u32)])
                .unwrap_or_default()
        } else {
            index
                .children(key.node)
                .ok_or_else(|| Self::error("syntax children unavailable"))?
                .iter()
                .rev()
                .map(|id| (*id, 1u32))
                .collect()
        };
        let mut result = Vec::new();
        let mut seen = HashSet::new();
        while let Some((node, depth)) = pending.pop() {
            if depth > bounds.max_depth || result.len() >= bounds.max_nodes as usize || !seen.insert(node) {
                return Err(Self::error("syntax traversal exceeds bounds or index contains a cycle"));
            }
            let metadata =
                index.metadata_for(key.generation, node).ok_or_else(|| Self::error("syntax metadata unavailable"))?;
            result.push(AstNodeKey { node, ..key });
            if ancestors {
                if let Some(parent) = metadata.parent {
                    pending.push((parent, depth + 1));
                }
            } else {
                for child in
                    index.children(node).ok_or_else(|| Self::error("syntax children unavailable"))?.iter().rev()
                {
                    pending.push((*child, depth + 1));
                }
            }
        }
        Ok(result)
    }
    fn host_item(
        &self,
        root: AstNodeKey,
        target: AstNodeKey,
        bounds: ModSyntaxBounds,
    ) -> Result<usize, ModSemanticError> {
        let entry = self.assembly.entry_unit();
        if root.unit != SourceUnitId::new(self.db, entry.path.clone())
            || root.node != AstNodeId(0)
            || target.unit != root.unit
            || self.assembly.package_identities().for_source(&entry.path).is_some()
            || self.assembly.trusted_corelib_service_paths.contains(&entry.path)
        {
            return Err(Self::error("syntax mutation requires actual host-owned entry program"));
        }
        self.syntax_walk(root, bounds, false)?;
        let syntax = self.db.syntax_unit(root.unit).ok_or_else(|| Self::error("syntax unit unavailable"))?;
        let index = syntax.syntax_index(self.db);
        let program = syntax.expanded_program(self.db);
        for (ordinal, item) in program.node.items.iter().enumerate() {
            for child in index.children(root.node).ok_or_else(|| Self::error("program children unavailable"))? {
                let actual = index.node_at(program, *child).and_then(|node| node.of::<Node>());
                if actual.is_some_and(|actual| std::ptr::eq(actual, &item.node)) {
                    let direct =
                        index.metadata_for(target.generation, target.node).and_then(|m| m.parent) == Some(*child);
                    if target.node == *child || direct {
                        return Ok(ordinal);
                    }
                }
            }
        }
        Err(Self::error("pipeline target is not an editable typed program item"))
    }
    fn queue_edit(
        &self,
        issuer: u64,
        root: &ModSyntaxNodeRef,
        target: &ModSyntaxNodeRef,
        payload: Option<&ModSyntaxNodeRef>,
        bounds: ModSyntaxBounds,
        kind: EditKind,
    ) -> Result<ModSyntaxResponse, ModSemanticError> {
        let root_key = self.syntax_key(issuer, root)?;
        let target_key = self.syntax_key(issuer, target)?;
        let index = self.host_item(root_key, target_key, bounds)?;
        let (payload, payload_doc) = if let Some(payload) = payload {
            let key = self.syntax_key(issuer, payload)?;
            let ordinal = self.host_item(root_key, key, bounds)?;
            (
                Some(self.assembly.entry_unit().program.node.items[ordinal].clone()),
                self.assembly.entry_unit().program.node.leading_docs.get(ordinal).cloned().flatten(),
            )
        } else {
            (None, None)
        };
        let mut states = self.syntax_state.borrow_mut();
        let state = states.invocations.get_mut(&issuer).ok_or_else(|| Self::error("syntax invocation unavailable"))?;
        if state.applied.is_some() || state.edits.len() >= ITEMS || state.edits.iter().any(|edit| edit.index == index) {
            return Err(Self::error("pipeline is applied, conflicting or exceeds edit budget"));
        }
        state.edits.push(Edit { index, kind, payload, payload_doc });
        Ok(ModSyntaxResponse::Node(Some(root.clone())))
    }
    fn queue_projection(
        &self,
        issuer: u64,
        root: &ModSyntaxNodeRef,
        target: &ModSyntaxNodeRef,
        kind: beskid_analysis::syntax_query::NodeKind,
        value: &serde_json::Value,
        bounds: ModSyntaxBounds,
    ) -> Result<ModSyntaxResponse, ModSemanticError> {
        use beskid_analysis::syntax::{AstNodeId, Spanned};
        use beskid_analysis::syntax_query::{DynNodeRef, NodeKind};
        let bounds = bounds.checked()?;
        let root_key = self.syntax_key(issuer, root)?;
        let target_key = self.syntax_key(issuer, target)?;
        let ordinal = self.host_item(root_key, target_key, bounds)?;
        let syntax = self.db.syntax_unit(target_key.unit).ok_or_else(|| Self::error("syntax unit unavailable"))?;
        // Preflight the owned transport before recursive typed deserialization.
        let mut pending = vec![(value, 0usize)];
        let mut count = 0usize;
        while let Some((part, depth)) = pending.pop() {
            count += 1;
            if count > 1_000_000 || depth > 256 {
                return Err(Self::error("replacement transport budget exceeded"));
            }
            match part {
                serde_json::Value::Array(items) => pending.extend(items.iter().map(|item| (item, depth + 1))),
                serde_json::Value::Object(items) => pending.extend(items.values().map(|item| (item, depth + 1))),
                _ => {}
            }
        }
        if serde_json::to_vec(value).map_err(|e| Self::error(&e.to_string()))?.len() > 16 * 1024 * 1024 {
            return Err(Self::error("replacement byte budget exceeded"));
        }
        let original = &self.assembly.entry_unit().program.node.items[ordinal];
        let replacement_span = syntax
            .syntax_index(self.db)
            .metadata_for(target_key.generation, target_key.node)
            .and_then(|m| m.span)
            .unwrap_or(original.span);
        macro_rules! declaration {
            ($variant:ident) => {{
                let replacement = serde_json::from_value(value.clone()).map_err(|e| Self::error(&e.to_string()))?;
                Node::$variant(Spanned::new(replacement, replacement_span))
            }};
        }
        let node = match kind {
            NodeKind::Node => serde_json::from_value::<Node>(value.clone()).map_err(|e| Self::error(&e.to_string()))?,
            NodeKind::HostDefinition => declaration!(HostDefinition),
            NodeKind::FunctionDefinition => declaration!(Function),
            NodeKind::ConstantDefinition => declaration!(ConstantDefinition),
            NodeKind::MethodDefinition => declaration!(Method),
            NodeKind::ImplBlock => declaration!(ImplBlock),
            NodeKind::ExtendTypeDefinition => declaration!(ExtendTypeDefinition),
            NodeKind::MacroDefinition => declaration!(MacroDefinition),
            NodeKind::TypeDefinition => declaration!(TypeDefinition),
            NodeKind::EnumDefinition => declaration!(EnumDefinition),
            NodeKind::ContractDefinition => declaration!(ContractDefinition),
            NodeKind::TestDefinition => declaration!(TestDefinition),
            NodeKind::AttributeDeclaration => declaration!(AttributeDeclaration),
            NodeKind::ModuleDeclaration => declaration!(ModuleDeclaration),
            NodeKind::InlineModule => declaration!(InlineModule),
            NodeKind::UseDeclaration => declaration!(UseDeclaration),
            _ => return Err(Self::error("replacement kind is not permitted in a program item slot")),
        };
        let mut nodes = vec![(DynNodeRef::from(&node), 0u32)];
        let mut total = 0u32;
        while let Some((node, depth)) = nodes.pop() {
            total += 1;
            if total > bounds.max_nodes || depth > bounds.max_depth {
                return Err(Self::error("replacement AST budget exceeded"));
            }
            node.children(|child| nodes.push((child, depth + 1)));
        }
        // Payload identities are data only. The applied program is re-registered with
        // fresh generation authority; preserve the host item's outer provenance.
        let canonical = if kind == NodeKind::Node {
            serde_json::to_value(&node).map_err(|e| Self::error(&e.to_string()))?
        } else {
            let mut children = Vec::new();
            DynNodeRef::from(&node).children(|child| children.push(child));
            let child = children.first().ok_or_else(|| Self::error("replacement declaration unavailable"))?;
            child.0.owned_projection().map_err(|e| Self::error(&e.to_string()))?
        };
        if canonical != *value {
            return Err(Self::error("replacement is not a canonical owned AST projection"));
        }
        let mut payload = Spanned::new(node, original.span);
        payload.id = AstNodeId::INVALID;
        let mut states = self.syntax_state.borrow_mut();
        let state = states.invocations.get_mut(&issuer).ok_or_else(|| Self::error("syntax invocation unavailable"))?;
        if state.applied.is_some() || state.edits.len() >= ITEMS || state.edits.iter().any(|edit| edit.index == ordinal)
        {
            return Err(Self::error("pipeline is applied, conflicting or exceeds edit budget"));
        }
        state.edits.push(Edit {
            index: ordinal,
            kind: EditKind::Replace,
            payload: Some(payload),
            payload_doc: self.assembly.entry_unit().program.node.leading_docs.get(ordinal).cloned().flatten(),
        });
        Ok(ModSyntaxResponse::Node(Some(root.clone())))
    }
    pub(crate) fn take_all_applied_syntax(&self) -> Vec<ModAppliedSyntax> {
        let mut state = self.syntax_state.borrow_mut();
        self.catchalls.borrow_mut().clear();
        let invocations = std::mem::take(&mut state.invocations);
        invocations
            .into_iter()
            .filter_map(|(issuer, invocation)| {
                state.closed.insert(issuer);
                invocation.applied
            })
            .collect()
    }
}
impl ModSyntaxAuthority for ModSemanticQueryAuthority<'_> {
    fn take_applied_syntax(&self, issuer: u64) -> Result<Vec<ModAppliedSyntax>, ModSemanticError> {
        if issuer == 0 {
            return Err(Self::error("zero syntax invocation issuer"));
        }
        let mut state = self.syntax_state.borrow_mut();
        if state.closed.contains(&issuer) {
            return Err(Self::error("syntax invocation already closed"));
        }
        if state.closed.len() >= 4096 {
            return Err(Self::error("closed syntax invocation budget exceeded"));
        }
        state.closed.insert(issuer);
        self.catchalls.borrow_mut().retain(|(invocation, _), _| *invocation != issuer);
        Ok(state.invocations.remove(&issuer).and_then(|state| state.applied).into_iter().collect())
    }
    fn syntax_query(&self, issuer: u64, request: &ModSyntaxRequest) -> Result<ModSyntaxResponse, ModSemanticError> {
        self.validate_registration()?;
        if issuer == 0 {
            return Err(Self::error("zero syntax invocation issuer"));
        }
        if self.syntax_state.borrow().closed.contains(&issuer) {
            return Err(Self::error("syntax invocation is closed"));
        }
        let root_key = AstNodeKey {
            unit: SourceUnitId::new(self.db, self.assembly.entry_unit().path.clone()),
            generation: self.assembly.generation,
            node: AstNodeId(0),
        };
        let issue_nodes = |keys: Vec<AstNodeKey>| {
            keys.into_iter().map(|key| self.syntax_ref(issuer, key)).collect::<Result<Vec<_>, _>>()
        };
        match request {
            ModSyntaxRequest::Root => Ok(ModSyntaxResponse::Node(Some(self.syntax_ref(issuer, root_key)?))),
            ModSyntaxRequest::Children { node } => {
                let key = self.syntax_key(issuer, node)?;
                let syntax = self.db.syntax_unit(key.unit).ok_or_else(|| Self::error("syntax unit unavailable"))?;
                let children = syntax
                    .syntax_index(self.db)
                    .children(key.node)
                    .ok_or_else(|| Self::error("syntax children unavailable"))?;
                if children.len() > ITEMS {
                    return Err(Self::error("syntax child output exceeds budget"));
                }
                Ok(ModSyntaxResponse::Nodes(issue_nodes(
                    children.iter().map(|node| AstNodeKey { node: *node, ..key }).collect(),
                )?))
            }
            ModSyntaxRequest::Parent { node } => {
                let key = self.syntax_key(issuer, node)?;
                let syntax = self.db.syntax_unit(key.unit).ok_or_else(|| Self::error("syntax unit unavailable"))?;
                let parent = syntax.syntax_index(self.db).metadata_for(key.generation, key.node).and_then(|m| m.parent);
                Ok(ModSyntaxResponse::Node(
                    parent.map(|node| self.syntax_ref(issuer, AstNodeKey { node, ..key })).transpose()?,
                ))
            }
            ModSyntaxRequest::Ancestors { node, bounds } | ModSyntaxRequest::Descendants { node, bounds } => {
                let key = self.syntax_key(issuer, node)?;
                Ok(ModSyntaxResponse::Nodes(issue_nodes(self.syntax_walk(
                    key,
                    *bounds,
                    matches!(request, ModSyntaxRequest::Ancestors { .. }),
                )?)?))
            }
            ModSyntaxRequest::Span { node } => {
                let key = self.syntax_key(issuer, node)?;
                let syntax = self.db.syntax_unit(key.unit).ok_or_else(|| Self::error("syntax unit unavailable"))?;
                Ok(ModSyntaxResponse::Span(
                    syntax.syntax_index(self.db).metadata_for(key.generation, key.node).and_then(|m| m.span),
                ))
            }
            ModSyntaxRequest::OfKind { node, bounds, kind } | ModSyntaxRequest::FindFirst { node, bounds, kind } => {
                let key = self.syntax_key(issuer, node)?;
                let syntax = self.db.syntax_unit(key.unit).ok_or_else(|| Self::error("syntax unit unavailable"))?;
                let mut keys = vec![key];
                keys.extend(self.syntax_walk(key, *bounds, false)?);
                let found = keys
                    .into_iter()
                    .filter(|key| syntax.syntax_index(self.db).kind(key.node) == Some(*kind))
                    .collect::<Vec<_>>();
                if matches!(request, ModSyntaxRequest::FindFirst { .. }) {
                    Ok(ModSyntaxResponse::Node(found.first().map(|key| self.syntax_ref(issuer, *key)).transpose()?))
                } else {
                    Ok(ModSyntaxResponse::Nodes(issue_nodes(found)?))
                }
            }
            ModSyntaxRequest::Project { node, kind } => {
                let key = self.syntax_key(issuer, node)?;
                let syntax = self.db.syntax_unit(key.unit).ok_or_else(|| Self::error("syntax unit unavailable"))?;
                let index = syntax.syntax_index(self.db);
                let value = if index.kind(key.node) == Some(*kind) {
                    self.syntax_walk(key, ModSyntaxBounds { max_nodes: 65536, max_depth: 128 }, false)?;
                    let actual = index
                        .node_at(syntax.expanded_program(self.db), key.node)
                        .ok_or_else(|| Self::error("syntax projection unavailable"))?;
                    let value = actual.0.owned_projection().map_err(|e| Self::error(&e.to_string()))?;
                    if serde_json::to_vec(&value).map_err(|e| Self::error(&e.to_string()))?.len() > 16 * 1024 * 1024 {
                        return Err(Self::error("syntax projection byte budget exceeded"));
                    }
                    Some(value)
                } else {
                    None
                };
                Ok(ModSyntaxResponse::Projection { kind: *kind, value })
            }
            ModSyntaxRequest::Replace { root, target, replacement, bounds } => {
                self.queue_edit(issuer, root, target, Some(replacement), *bounds, EditKind::Replace)
            }
            ModSyntaxRequest::ReplaceProjection { root, target, kind, value, bounds } => {
                self.queue_projection(issuer, root, target, *kind, value, *bounds)
            }
            ModSyntaxRequest::Remove { root, target, bounds } => {
                self.queue_edit(issuer, root, target, None, *bounds, EditKind::Remove)
            }
            ModSyntaxRequest::InsertBefore { root, anchor, node, bounds } => {
                self.queue_edit(issuer, root, anchor, Some(node), *bounds, EditKind::Before)
            }
            ModSyntaxRequest::InsertAfter { root, anchor, node, bounds } => {
                self.queue_edit(issuer, root, anchor, Some(node), *bounds, EditKind::After)
            }
            ModSyntaxRequest::Apply { root, bounds } => {
                let key = self.syntax_key(issuer, root)?;
                bounds.checked()?;
                if key != root_key {
                    return Err(Self::error("pipeline apply requires actual host entry root"));
                }
                self.syntax_walk(key, *bounds, false)?;
                let mut states = self.syntax_state.borrow_mut();
                let state =
                    states.invocations.get_mut(&issuer).ok_or_else(|| Self::error("syntax invocation unavailable"))?;
                if state.applied.is_some() {
                    return Err(Self::error("pipeline already applied"));
                }
                let mut program = self.assembly.entry_unit().program.clone();
                if program.node.leading_docs.len() != program.node.items.len() {
                    return Err(Self::error("program document/item correspondence unavailable"));
                }
                let insertions =
                    state.edits.iter().filter(|edit| matches!(edit.kind, EditKind::Before | EditKind::After)).count();
                if program.node.items.len().checked_add(insertions).is_none_or(|count| count > ITEMS) {
                    return Err(Self::error("pipeline item budget exceeded"));
                }
                state.edits.sort_by_key(|edit| edit.index);
                for edit in state.edits.iter().rev() {
                    match edit.kind {
                        EditKind::Remove => {
                            program.node.items.remove(edit.index);
                            program.node.leading_docs.remove(edit.index);
                        }
                        EditKind::Replace => {
                            program.node.items[edit.index] =
                                edit.payload.clone().ok_or_else(|| Self::error("pipeline replacement unavailable"))?;
                            program.node.leading_docs[edit.index] = edit.payload_doc.clone();
                        }
                        EditKind::Before | EditKind::After => {
                            let index = edit.index + usize::from(matches!(edit.kind, EditKind::After));
                            program.node.items.insert(
                                index,
                                edit.payload.clone().ok_or_else(|| Self::error("pipeline insertion unavailable"))?,
                            );
                            program.node.leading_docs.insert(index, edit.payload_doc.clone());
                        }
                    }
                }
                state.applied = Some(ModAppliedSyntax {
                    source_unit: self.assembly.entry_unit().path.clone(),
                    previous_generation: key.generation,
                    program,
                });
                state.edits.clear();
                Ok(ModSyntaxResponse::Node(Some(root.clone())))
            }
        }
    }
}
