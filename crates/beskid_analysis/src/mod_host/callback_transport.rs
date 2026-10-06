//! Bounded synchronous semantic transport. The query issuer stays borrowed on this thread.
//! A worker receives only framed values and invocation tokens, never a Rust authority pointer.
use super::{
    ModSemanticAuthority, ModSemanticDeclaration, ModSemanticFieldType, ModSemanticHandle, ModSemanticShape,
    ModSemanticShapeBody,
};
use crate::syntax::{AstNodeId, SpanInfo, SyntaxGenerationId};
use anyhow::{Result, bail};
use serde::Deserialize;
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT_INVOCATION: AtomicU64 = AtomicU64::new(1);
const REQUEST_BYTES: usize = 65536;
const RESPONSE_BYTES: usize = 16 * 1024 * 1024;
const TOTAL_BYTES: usize = 64 * 1024 * 1024;
const REQUEST_LIMIT: u64 = 65536;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u32,
    invocation: u64,
    sequence: u64,
    operation: Operation,
}
#[derive(Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum Operation {
    ResolveType { source_unit: String, generation: u64, node: u32, span: WireSpan },
    ResolveSyntaxType { reference: super::ModSyntaxNodeRef },
    ResolveSyntaxTemplate { reference: super::ModSyntaxNodeRef },
    PlanCanonicalPaths { reference: super::ModSyntaxNodeRef, requested: Vec<String> },
    ResolveFunction { reference: super::ModSyntaxNodeRef, path: Vec<String> },
    CheckSerializable { handle: u64 },
    TypeShape { handle: u64 },
    CaptureCatchall { owner: u64, source_unit: String, generation: u64, node: u32, span: WireSpan },
    ValidateCatchall { token: u64 },
    Syntax { request: super::ModSyntaxRequest },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireSpan {
    start: u64,
    end: u64,
    line_start: u64,
    column_start: u64,
    line_end: u64,
    column_end: u64,
}
impl WireSpan {
    fn checked(self) -> Result<SpanInfo> {
        if self.start > self.end
            || self.line_start == 0
            || self.line_end == 0
            || self.column_start == 0
            || self.column_end == 0
            || (self.line_end, self.column_end) < (self.line_start, self.column_start)
        {
            bail!("invalid native semantic source span");
        }
        Ok(SpanInfo {
            start: usize::try_from(self.start)?,
            end: usize::try_from(self.end)?,
            line_col_start: (usize::try_from(self.line_start)?, usize::try_from(self.column_start)?),
            line_col_end: (usize::try_from(self.line_end)?, usize::try_from(self.column_end)?),
        })
    }
}
/// The lifetime and lack of Send/Sync are inherited from the actual query authority.
/// No constructor can create a session without a borrowed semantic issuer.
pub(crate) struct NativeSemanticSession<'a> {
    authority: &'a dyn ModSemanticAuthority,
    invocation: u64,
    next_sequence: u64,
    bytes: usize,
    closed: bool,
    compiled_metadata: Vec<super::ModCompiledMetadata>,
}
impl<'a> NativeSemanticSession<'a> {
    pub(crate) fn new(authority: &'a dyn ModSemanticAuthority) -> Result<Self> {
        let invocation = NEXT_INVOCATION
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| value.checked_add(1))
            .map_err(|_| anyhow::anyhow!("native semantic invocation identities exhausted"))?;
        Ok(Self { authority, invocation, next_sequence: 1, bytes: 0, closed: false, compiled_metadata: Vec::new() })
    }
    pub(crate) fn compile_target(
        &self,
        index: u32,
        token: u64,
        item: &crate::syntax::Spanned<super::ProgramItem>,
    ) -> Result<super::ModCompiledMetadata> {
        if self.closed || token == 0 {
            bail!("serialization target invocation is closed or target is absent");
        }
        self.authority
            .compile_serialization_target(self.invocation, index, super::ModSemanticHandle::from_token(token), item)
            .map_err(|e| anyhow::anyhow!(e.to_string()))
    }
    pub(crate) fn compile_template(
        &self,
        index: u32,
        reference: &super::ModSyntaxNodeRef,
        item: &crate::syntax::Spanned<super::ProgramItem>,
    ) -> Result<super::ModCompiledMetadata> {
        if self.closed {
            bail!("serialization template invocation is closed");
        }
        let super::ModSyntaxResponse::Span(Some(span)) = self
            .authority
            .syntax_query(self.invocation, &super::ModSyntaxRequest::Span { node: reference.clone() })
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
        else {
            bail!("template declaration span unavailable");
        };
        let declaration = ModSemanticDeclaration {
            source_unit: reference.source_unit.clone(),
            generation: reference.generation,
            node: reference.node,
            span,
        };
        self.authority
            .compile_serialization_template(self.invocation, index, reference, &declaration, item)
            .map_err(|e| anyhow::anyhow!(e.to_string()))
    }
    pub(crate) fn take_compiled_metadata(&mut self) -> Vec<super::ModCompiledMetadata> {
        std::mem::take(&mut self.compiled_metadata)
    }
    pub(crate) fn syntax_query(&self, request: &super::ModSyntaxRequest) -> Result<super::ModSyntaxResponse> {
        self.authority.syntax_query(self.invocation, request).map_err(|e| anyhow::anyhow!(e.to_string()))
    }
    pub(crate) fn invocation(&self) -> u64 {
        self.invocation
    }
    pub(crate) fn generation(&self) -> u64 {
        self.authority.generation().0
    }
    /// The root comes from the current registered issuer, never a caller node ID.
    pub(crate) fn source_root_value(&self) -> Result<serde_json::Value> {
        let response = self
            .authority
            .syntax_query(self.invocation, &super::ModSyntaxRequest::Root)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let super::ModSyntaxResponse::Node(Some(node)) = response else {
            bail!("native invocation has no registered entry syntax root");
        };
        if node.invocation_issuer != self.invocation || node.generation.0 != self.generation() {
            bail!("native invocation root has foreign issuer or generation");
        }
        let source = node.source_unit.to_str().ok_or_else(|| anyhow::anyhow!("native root path is not UTF8"))?;
        Ok(serde_json::json!({"sourceUnit":source,"invocationIssuer":node.invocation_issuer,
            "syntaxGenerationId":node.generation.0,"nodeId":node.node.0}))
    }

    pub(crate) fn handle_frame(&mut self, frame: &[u8]) -> Result<Vec<u8>> {
        if self.closed {
            bail!("native semantic invocation is closed");
        }
        let result = self.handle_frame_inner(frame);
        if result.is_err() {
            self.closed = true;
        }
        result
    }
    fn handle_frame_inner(&mut self, frame: &[u8]) -> Result<Vec<u8>> {
        if frame.is_empty() || frame.len() > REQUEST_BYTES {
            bail!("native semantic request exceeds frame bounds");
        }
        self.bytes = self
            .bytes
            .checked_add(frame.len())
            .ok_or_else(|| anyhow::anyhow!("native semantic byte budget exhausted"))?;
        if self.bytes > TOTAL_BYTES || self.next_sequence > REQUEST_LIMIT {
            bail!("native semantic invocation work budget exhausted");
        }
        let request: Request = serde_json::from_slice(frame)?;
        if request.version != 2 || request.invocation != self.invocation || request.sequence != self.next_sequence {
            bail!("foreign, expired, reordered or replayed native semantic invocation");
        }
        self.next_sequence += 1;
        let response = match request.operation {
            Operation::PlanCanonicalPaths { reference, requested } => {
                if requested.len() > 64 || requested.iter().any(|name| name.is_empty() || name.len() > 256) {
                    bail!("canonical path selectors exceed bounds");
                }
                let paths = self
                    .authority
                    .plan_canonical_paths(self.invocation, &reference, &requested)
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                if paths.len() != requested.len()
                    || paths.iter().zip(&requested).any(|(path, request)| {
                        path.logical_name != *request
                            || path.segments.is_empty()
                            || path.segments.len() > 128
                            || path.segments.iter().any(|segment| segment.is_empty() || segment.len() > 1024)
                    })
                {
                    bail!("canonical path provider response violates correspondence/bounds");
                }
                serde_json::json!({"version":2,"sequence":request.sequence,"paths":paths})
            }
            Operation::ResolveFunction { reference, path } => {
                if !function_path_in_bounds(&path) {
                    bail!("function path exceeds bounds or is not an identifier route");
                }
                match self.authority.resolve_function(self.invocation, &reference, &path) {
                    Ok(signature) => {
                        validate_function_budget(&signature)?;
                        serde_json::json!({"version":2,"sequence":request.sequence,"function":signature})
                    }
                    Err(error) => {
                        serde_json::json!({"version":2,"sequence":request.sequence,"error":error.to_string()})
                    }
                }
            }
            Operation::CheckSerializable { handle } => {
                match self.authority.check_serializable(ModSemanticHandle::from_token(handle)) {
                    Ok(()) => serde_json::json!({"version":2,"sequence":request.sequence,"serializable":true}),
                    Err(error) => {
                        serde_json::json!({"version":2,"sequence":request.sequence,"error":error.to_string()})
                    }
                }
            }
            Operation::ResolveSyntaxTemplate { reference } => {
                self.authority
                    .syntax_query(self.invocation, &super::ModSyntaxRequest::Span { node: reference.clone() })
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                let mut generic = false;
                for kind in
                    [crate::syntax_query::NodeKind::TypeDefinition, crate::syntax_query::NodeKind::EnumDefinition]
                {
                    let projection = self
                        .authority
                        .syntax_query(
                            self.invocation,
                            &super::ModSyntaxRequest::Project { node: reference.clone(), kind },
                        )
                        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                    if let super::ModSyntaxResponse::Projection { value: Some(value), .. } = projection {
                        generic = value
                            .get("generics")
                            .and_then(Value::as_array)
                            .is_some_and(|parameters| !parameters.is_empty());
                        break;
                    }
                }
                if !generic {
                    bail!("issued syntax reference is not a generic nominal template");
                }
                serde_json::json!({"version":2,"sequence":request.sequence,"template":reference})
            }
            Operation::ResolveSyntaxType { reference } => {
                // Span is issued only after the provider validates this exact nonce/key.
                let response = self
                    .authority
                    .syntax_query(self.invocation, &super::ModSyntaxRequest::Span { node: reference.clone() })
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                let super::ModSyntaxResponse::Span(Some(span)) = response else {
                    bail!("issued syntax reference has no canonical declaration span");
                };
                let mut nominal = false;
                for kind in
                    [crate::syntax_query::NodeKind::TypeDefinition, crate::syntax_query::NodeKind::EnumDefinition]
                {
                    let projection = self
                        .authority
                        .syntax_query(
                            self.invocation,
                            &super::ModSyntaxRequest::Project { node: reference.clone(), kind },
                        )
                        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                    if let super::ModSyntaxResponse::Projection { value: Some(value), .. } = projection {
                        let generics = value
                            .get("generics")
                            .and_then(serde_json::Value::as_array)
                            .ok_or_else(|| anyhow::anyhow!("canonical nominal projection has no generic metadata"))?;
                        if !generics.is_empty() {
                            bail!("unapplied generic declaration requires template authority");
                        }
                        nominal = true;
                        break;
                    }
                }
                if !nominal {
                    bail!("issued syntax reference is not a concrete nominal declaration");
                }
                let declaration = ModSemanticDeclaration {
                    source_unit: reference.source_unit,
                    generation: reference.generation,
                    node: reference.node,
                    span,
                };
                match self.authority.resolve_declaration(&declaration) {
                    Ok(handle) => serde_json::json!({"version":2,"sequence":request.sequence,"handle":handle.token()}),
                    Err(error) => {
                        serde_json::json!({"version":2,"sequence":request.sequence,"error":error.to_string()})
                    }
                }
            }
            Operation::ResolveType { source_unit, generation, node, span } => {
                if source_unit.is_empty()
                    || source_unit.len() > 8192
                    || source_unit.contains('\0')
                    || generation != self.generation()
                {
                    bail!("invalid or stale native semantic declaration");
                }
                let declaration = ModSemanticDeclaration {
                    source_unit: PathBuf::from(source_unit),
                    generation: SyntaxGenerationId(generation),
                    node: AstNodeId(node),
                    span: span.checked()?,
                };
                match self.authority.resolve_declaration(&declaration) {
                    Ok(handle) => serde_json::json!({"version":2,"sequence":request.sequence,"handle":handle.token()}),
                    Err(error) => {
                        serde_json::json!({"version":2,"sequence":request.sequence,"error":error.to_string()})
                    }
                }
            }
            Operation::CaptureCatchall { owner, source_unit, generation, node, span } => {
                if source_unit.is_empty()
                    || source_unit.len() > 8192
                    || source_unit.contains('\0')
                    || generation != self.generation()
                {
                    bail!("invalid or stale native catchall field declaration");
                }
                let declaration = ModSemanticDeclaration {
                    source_unit: PathBuf::from(source_unit),
                    generation: SyntaxGenerationId(generation),
                    node: AstNodeId(node),
                    span: span.checked()?,
                };
                match self.authority.issue_catchall(self.invocation, ModSemanticHandle::from_token(owner), &declaration)
                {
                    Ok(claim) => {
                        self.authority
                            .validate_catchall_claim(self.invocation, &claim)
                            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                        self.compiled_metadata.push(
                            self.authority
                                .compile_catchall(self.invocation, &claim)
                                .map_err(|error| anyhow::anyhow!(error.to_string()))?,
                        );
                        serde_json::json!({"version":2,"sequence":request.sequence,"catchall":claim})
                    }
                    Err(error) => {
                        serde_json::json!({"version":2,"sequence":request.sequence,"error":error.to_string()})
                    }
                }
            }
            Operation::ValidateCatchall { token } => match self.authority.lookup_catchall(self.invocation, token) {
                Ok(claim) => {
                    self.authority
                        .validate_catchall_claim(self.invocation, &claim)
                        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                    serde_json::json!({"version":2,"sequence":request.sequence,"catchall":claim})
                }
                Err(error) => serde_json::json!({"version":2,"sequence":request.sequence,"error":error.to_string()}),
            },
            Operation::Syntax { request: syntax } => match self.authority.syntax_query(self.invocation, &syntax) {
                Ok(value) => serde_json::json!({"version":2,"sequence":request.sequence,"syntax":value}),
                Err(error) => serde_json::json!({"version":2,"sequence":request.sequence,"error":error.to_string()}),
            },
            Operation::TypeShape { handle } => match self.authority.type_shape(ModSemanticHandle::from_token(handle)) {
                Ok(shape) => {
                    validate_shape_budget(&shape)?;
                    serde_json::json!({"version":2,"sequence":request.sequence,"shape":shape})
                }
                Err(error) => serde_json::json!({"version":2,"sequence":request.sequence,"error":error.to_string()}),
            },
        };
        let bytes = serde_json::to_vec(&response)?;
        if bytes.len() > RESPONSE_BYTES {
            bail!("native semantic response exceeds frame bounds");
        }
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .ok_or_else(|| anyhow::anyhow!("native semantic byte budget exhausted"))?;
        if self.bytes > TOTAL_BYTES {
            bail!("native semantic invocation byte budget exhausted");
        }
        Ok(bytes)
    }
}
/// A function route is a short identifier path; it is never a filesystem or symbol string.
pub(crate) fn function_path_in_bounds(path: &[String]) -> bool {
    !path.is_empty()
        && path.len() <= 64
        && path.iter().all(|segment| {
            let bytes = segment.as_bytes();
            !bytes.is_empty()
                && bytes.len() <= 256
                && (bytes[0] == b'_' || bytes[0].is_ascii_alphabetic())
                && bytes.iter().all(|byte| *byte == b'_' || byte.is_ascii_alphanumeric())
        })
}
fn validate_function_budget(signature: &super::ModSemanticFunctionSignature) -> Result<()> {
    fn depth(ty: &ModSemanticFieldType, level: usize) -> Result<()> {
        if level > 128 {
            bail!("function signature exceeds invocation type depth");
        }
        if let ModSemanticFieldType::Array(element) = ty {
            depth(element, level + 1)?;
        }
        Ok(())
    }
    if signature.parameters.len() > 65536
        || signature.declaration.source_unit.to_str().is_none_or(|path| path.is_empty() || path.len() > 8192)
    {
        bail!("function signature exceeds invocation structural bounds");
    }
    for ty in signature.parameters.iter().chain(std::iter::once(&signature.result)) {
        depth(ty, 0)?;
    }
    Ok(())
}
fn validate_shape_budget(shape: &ModSemanticShape) -> Result<()> {
    struct Budget {
        nodes: usize,
        text: usize,
    }
    impl Budget {
        fn add(&mut self, nodes: usize, text: usize) -> Result<()> {
            self.nodes =
                self.nodes.checked_add(nodes).ok_or_else(|| anyhow::anyhow!("semantic node budget exhausted"))?;
            self.text = self.text.checked_add(text).ok_or_else(|| anyhow::anyhow!("semantic text budget exhausted"))?;
            if self.nodes > 65536 || self.text > 8 * 1024 * 1024 {
                bail!("semantic shape exceeds invocation structural bounds");
            }
            Ok(())
        }
        fn declaration(&mut self, declaration: &ModSemanticDeclaration) -> Result<()> {
            let path =
                declaration.source_unit.to_str().ok_or_else(|| anyhow::anyhow!("semantic source path is not UTF8"))?;
            self.add(1, path.len())
        }
        fn ty(&mut self, ty: &ModSemanticFieldType, depth: usize) -> Result<()> {
            if depth > 128 {
                bail!("semantic shape exceeds invocation type depth");
            }
            self.add(1, 0)?;
            if let ModSemanticFieldType::Array(element) = ty {
                self.ty(element, depth + 1)?;
            }
            Ok(())
        }
        fn fields(&mut self, fields: &[super::ModSemanticField]) -> Result<()> {
            self.add(fields.len(), 0)?;
            for field in fields {
                self.add(0, field.name.len())?;
                self.declaration(&field.declaration)?;
                self.ty(&field.ty, 0)?;
            }
            Ok(())
        }
    }
    let mut budget = Budget { nodes: 0, text: 0 };
    budget.add(1, shape.name.len())?;
    budget.add(0, shape.declaration_identity.len())?;
    budget.declaration(&shape.declaration)?;
    if let Some(declaration) = &shape.package_declaration {
        budget.add(declaration.lexical_path.len(), declaration.source_path.len())?;
        for component in &declaration.lexical_path {
            budget.add(0, component.len())?;
        }
    }
    if let Some(package) = &shape.package {
        budget.add(1, package.package_name().len())?;
        budget.add(0, package.version().len())?;
        budget.add(0, package.source_digest().len())?;
        if let crate::projects::VerifiedPackageSource::Registry { registry, artifact_digest } = package.source() {
            budget.add(0, registry.len())?;
            budget.add(0, artifact_digest.len())?;
        }
    }
    budget.add(shape.type_arguments.len(), 0)?;
    for ty in &shape.type_arguments {
        budget.ty(ty, 0)?;
    }
    match &shape.body {
        ModSemanticShapeBody::Record { fields } => budget.fields(fields)?,
        ModSemanticShapeBody::Enum { variants } => {
            budget.add(variants.len(), 0)?;
            for variant in variants {
                budget.add(0, variant.name.len())?;
                budget.declaration(&variant.declaration)?;
                budget.fields(&variant.fields)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    struct Authority {
        calls: Cell<u32>,
    }
    impl crate::mod_host::ModSyntaxAuthority for Authority {
        fn take_applied_syntax(
            &self,
            _issuer: u64,
        ) -> Result<Vec<crate::mod_host::ModAppliedSyntax>, crate::mod_host::ModSemanticError> {
            Ok(Vec::new())
        }
        fn syntax_query(
            &self,
            _issuer: u64,
            _request: &crate::mod_host::ModSyntaxRequest,
        ) -> Result<crate::mod_host::ModSyntaxResponse, crate::mod_host::ModSemanticError> {
            Err(crate::mod_host::ModSemanticError("test authority has no registered syntax assembly".into()))
        }
    }
    impl ModSemanticAuthority for Authority {
        fn generation(&self) -> SyntaxGenerationId {
            SyntaxGenerationId(91)
        }
        fn resolve_declaration(
            &self,
            _: &ModSemanticDeclaration,
        ) -> Result<ModSemanticHandle, super::super::ModSemanticError> {
            self.calls.set(self.calls.get() + 1);
            Err(super::super::ModSemanticError("foreign declaration".into()))
        }
        fn type_shape(&self, _: ModSemanticHandle) -> Result<ModSemanticShape, super::super::ModSemanticError> {
            self.calls.set(self.calls.get() + 1);
            Err(super::super::ModSemanticError("expired issuer token".into()))
        }
    }
    fn frame(session: &NativeSemanticSession<'_>, sequence: u64) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"version":2,"invocation":session.invocation(),"sequence":sequence,
            "operation":{"kind":"TypeShape","handle":17}}))
        .unwrap()
    }
    #[test]
    fn scoped_native_transport_rejects_replay_and_closes_without_reusing_authority() {
        let authority = Authority { calls: Cell::new(0) };
        let mut session = NativeSemanticSession::new(&authority).unwrap();
        let request = frame(&session, 1);
        let response: serde_json::Value = serde_json::from_slice(&session.handle_frame(&request).unwrap()).unwrap();
        assert_eq!(response["error"], "expired issuer token");
        assert_eq!(authority.calls.get(), 1);
        assert!(session.handle_frame(&request).is_err());
        assert!(session.handle_frame(&frame(&session, 2)).is_err());
        assert_eq!(authority.calls.get(), 1);
    }
    #[test]
    fn v06_resolve_syntax_type_validates_issued_reference_before_semantic_resolution() {
        let authority = Authority { calls: Cell::new(0) };
        let mut session = NativeSemanticSession::new(&authority).unwrap();
        let request = serde_json::to_vec(&serde_json::json!({
            "version":2,"invocation":session.invocation(),"sequence":1,
            "operation":{"kind":"ResolveSyntaxType","reference":{
                "source_unit":"Foreign.bd","invocation_issuer":session.invocation(),
                "generation":91,"node":7}}
        }))
        .unwrap();
        let error = session.handle_frame(&request).unwrap_err();
        assert!(error.to_string().contains("no registered syntax assembly"), "{error:#}");
        assert_eq!(authority.calls.get(), 0);
    }
    #[test]
    fn v06_template_transport_never_issues_concrete_semantic_handle() {
        struct TemplateAuthority {
            semantic_calls: Cell<u32>,
        }
        impl super::super::ModSyntaxAuthority for TemplateAuthority {
            fn take_applied_syntax(
                &self,
                _: u64,
            ) -> Result<Vec<super::super::ModAppliedSyntax>, super::super::ModSemanticError> {
                Ok(Vec::new())
            }
            fn syntax_query(
                &self,
                issuer: u64,
                request: &super::super::ModSyntaxRequest,
            ) -> Result<super::super::ModSyntaxResponse, super::super::ModSemanticError> {
                use super::super::{ModSyntaxRequest, ModSyntaxResponse};
                let reference = match request {
                    ModSyntaxRequest::Span { node } | ModSyntaxRequest::Project { node, .. } => node,
                    _ => return Err(super::super::ModSemanticError("unexpected test request".into())),
                };
                if reference.invocation_issuer != issuer || reference.generation.0 != 91 || reference.node.0 != 7 {
                    return Err(super::super::ModSemanticError("foreign test reference".into()));
                }
                match request {
                    ModSyntaxRequest::Span { .. } => Ok(ModSyntaxResponse::Span(Some(SpanInfo {
                        start: 0,
                        end: 20,
                        line_col_start: (1, 1),
                        line_col_end: (1, 21),
                    }))),
                    ModSyntaxRequest::Project { kind, .. } => Ok(ModSyntaxResponse::Projection {
                        kind: *kind,
                        value: Some(serde_json::json!({"generics":[{"node":{"name":"T"}}]})),
                    }),
                    _ => unreachable!(),
                }
            }
        }
        impl ModSemanticAuthority for TemplateAuthority {
            fn generation(&self) -> SyntaxGenerationId {
                SyntaxGenerationId(91)
            }
            fn resolve_declaration(
                &self,
                _: &ModSemanticDeclaration,
            ) -> Result<ModSemanticHandle, super::super::ModSemanticError> {
                self.semantic_calls.set(self.semantic_calls.get() + 1);
                Err(super::super::ModSemanticError("template cannot be a concrete handle".into()))
            }
            fn type_shape(&self, _: ModSemanticHandle) -> Result<ModSemanticShape, super::super::ModSemanticError> {
                Err(super::super::ModSemanticError("template cannot be a concrete shape".into()))
            }
        }
        let authority = TemplateAuthority { semantic_calls: Cell::new(0) };
        let mut session = NativeSemanticSession::new(&authority).unwrap();
        let reference = serde_json::json!({"source_unit":"Template.bd","invocation_issuer":session.invocation(),"generation":91,"node":7});
        let frame = serde_json::to_vec(&serde_json::json!({"version":2,"invocation":session.invocation(),"sequence":1,
            "operation":{"kind":"ResolveSyntaxTemplate","reference":reference}}))
        .unwrap();
        let output: serde_json::Value = serde_json::from_slice(&session.handle_frame(&frame).unwrap()).unwrap();
        assert_eq!(output["template"]["node"], 7);
        assert!(output.get("handle").is_none());
        let frame = serde_json::to_vec(&serde_json::json!({"version":2,"invocation":session.invocation(),"sequence":2,
            "operation":{"kind":"ResolveSyntaxType","reference":reference}}))
        .unwrap();
        assert!(session.handle_frame(&frame).unwrap_err().to_string().contains("requires template authority"));
        assert_eq!(authority.semantic_calls.get(), 0);
    }
    #[test]
    fn v06_function_and_serializable_transport_bounds_routes_and_reports_issuer_denial() {
        let authority = Authority { calls: Cell::new(0) };
        let resolve = |session: &NativeSemanticSession<'_>, path: serde_json::Value| {
            serde_json::to_vec(&serde_json::json!({"version":2,"invocation":session.invocation(),"sequence":1,
                "operation":{"kind":"ResolveFunction","reference":{"source_unit":"Main.bd",
                    "invocation_issuer":session.invocation(),"generation":91,"node":1},"path":path}}))
            .unwrap()
        };
        for invalid in [serde_json::json!([]), serde_json::json!(["../Escape"]), serde_json::json!(["1Leading"])] {
            let mut session = NativeSemanticSession::new(&authority).unwrap();
            assert!(session.handle_frame(&resolve(&session, invalid)).is_err());
        }
        let mut session = NativeSemanticSession::new(&authority).unwrap();
        let output: serde_json::Value =
            serde_json::from_slice(&session.handle_frame(&resolve(&session, serde_json::json!(["Defaults", "Make"]))).unwrap())
                .unwrap();
        assert!(output["error"].as_str().is_some_and(|error| error.contains("cannot resolve")), "{output}");
        assert!(output.get("function").is_none());
        let frame = serde_json::to_vec(&serde_json::json!({"version":2,"invocation":session.invocation(),"sequence":2,
            "operation":{"kind":"CheckSerializable","handle":17}}))
        .unwrap();
        let output: serde_json::Value = serde_json::from_slice(&session.handle_frame(&frame).unwrap()).unwrap();
        assert!(output["error"].as_str().is_some_and(|error| error.contains("serialization eligibility")), "{output}");
        assert!(output.get("serializable").is_none());
        assert_eq!(authority.calls.get(), 0, "resolution and the gate never fall back to shape queries");
    }
    #[test]
    fn foreign_and_oversized_transport_frames_never_touch_semantic_issuer() {
        let authority = Authority { calls: Cell::new(0) };
        let mut session = NativeSemanticSession::new(&authority).unwrap();
        let mut request: serde_json::Value = serde_json::from_slice(&frame(&session, 1)).unwrap();
        request["invocation"] = serde_json::json!(session.invocation() + 1);
        assert!(session.handle_frame(&serde_json::to_vec(&request).unwrap()).is_err());
        let mut another = NativeSemanticSession::new(&authority).unwrap();
        assert!(another.handle_frame(&vec![b' '; REQUEST_BYTES + 1]).is_err());
        assert_eq!(authority.calls.get(), 0);
    }
}
