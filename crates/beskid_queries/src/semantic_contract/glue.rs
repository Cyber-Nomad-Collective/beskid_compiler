//! Canonical generation-bound logical Glue bindings. Physical transport is emitted later.
use super::*;
use beskid_analysis::syntax::{
    Attribute, ContractMethodSignature, Expression, FunctionDefinition, Literal, Parameter, PrimitiveType, SpanInfo,
    Spanned, Type, TypeDefinition,
};
use beskid_analysis::syntax_query::NodeKind;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum GlueDirection {
    Import,
    Export,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GlueHandleBrand {
    pub declaration: AstNodeKey,
    pub qualified_identity: Arc<str>,
    pub library: String,
    pub nullable: bool,
    pub arguments: Arc<[GlueLogicalType]>,
    /// Declared `[RustOwner(Path:..)]` Rust type path. Required by the Rust owner build; see
    /// [`rust_owner_tables`].
    pub rust_owner: Option<Arc<str>>,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum GlueLogicalType {
    Primitive(PrimitiveType),
    Array(Box<GlueLogicalType>),
    Generic { declaration: AstNodeKey, position: u32, name: String },
    Handle(GlueHandleBrand),
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GlueParameterFact {
    pub name: String,
    pub ty: GlueLogicalType,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GlueBindingFact {
    pub declaration: AstNodeKey,
    pub direction: GlueDirection,
    pub symbol: String,
    pub library: Option<String>,
    pub generics: Arc<[String]>,
    pub parameters: Arc<[GlueParameterFact]>,
    pub result: GlueLogicalType,
    /// Rust owner callable mapping. Present for every import, with the declared or default
    /// `implementation::<symbol>` path and fallibility. Absent for exports.
    pub rust_owner: Option<GlueRustOwnerCallable>,
}
/// Rust owner callable mapping of one Extern contract method.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GlueRustOwnerCallable {
    pub path: Arc<str>,
    pub fallible: bool,
    /// True when the method carries an explicit `RustOwner` attribute.
    pub declared: bool,
}
fn invalid(message: &str) -> SemanticError {
    SemanticError::new(message)
}
fn text_attribute(attribute: &Attribute, name: &str) -> Result<Option<String>, SemanticError> {
    let Some(arg) = attribute.arguments.iter().find(|a| a.node.name.node.name == name) else { return Ok(None) };
    let Expression::Literal(literal) = &arg.node.value.node else {
        return Err(invalid("Glue metadata must be a literal"));
    };
    let Literal::String(raw) = &literal.node.literal.node else {
        return Err(invalid("Glue metadata must be a string literal"));
    };
    if raw.len() > 16384 {
        return Err(invalid("Glue metadata token exceeds bounded input"));
    }
    let value = beskid_analysis::syntax::decode_string_literal_token(raw)
        .map_err(|_| invalid("invalid Glue string metadata"))?;
    if value.is_empty() || value.contains('\0') || value.len() > 4096 {
        return Err(invalid("invalid or excessive Glue metadata"));
    }
    Ok(Some(value.into()))
}
fn closed_arguments(attribute: &Attribute, allowed: &[&str]) -> Result<(), SemanticError> {
    if attribute.arguments.len() > allowed.len() {
        return Err(invalid("duplicate or unknown Glue metadata argument"));
    }
    let mut seen = std::collections::HashSet::new();
    for arg in &attribute.arguments {
        let name = arg.node.name.node.name.as_str();
        if !allowed.contains(&name) || !seen.insert(name) {
            return Err(invalid("duplicate or unknown Glue metadata argument"));
        }
    }
    Ok(())
}
const RUST_OWNER: &str = "RustOwner";
/// Segments that cannot appear in a Rust owner path. A path resolves only inside the generated
/// `implementation` module, so crate roots, relative roots and Rust keywords are closed out.
const RUST_OWNER_RESERVED_SEGMENTS: &[&str] = &[
    "crate", "self", "Self", "super", "std", "core", "alloc", "as", "async", "await", "break", "const", "continue",
    "dyn", "else", "enum", "extern", "false", "fn", "for", "gen", "if", "impl", "in", "let", "loop", "match", "mod",
    "move", "mut", "pub", "ref", "return", "static", "struct", "trait", "true", "try", "type", "unsafe", "use",
    "where", "while", "abstract", "become", "box", "do", "final", "macro", "override", "priv", "typeof", "unsized",
    "virtual", "yield", "_",
];
/// The two closed `RustOwner` placements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum RustOwnerPlacement {
    /// `[RustOwner(Path:..)]` on a `GlueHandle` type. `Path` is required, `Fallible` is rejected.
    HandleType,
    /// `[RustOwner(Path:.., Fallible:..)]` on an `Extern` contract method. Both are optional.
    ExternMethod,
}
struct RustOwnerArguments {
    path: Option<String>,
    fallible: Option<bool>,
}
fn spanned(span: &SpanInfo, message: impl std::fmt::Display) -> SemanticError {
    invalid(&format!(
        "{message} at {}:{} (bytes {}..{})",
        span.line_col_start.0, span.line_col_start.1, span.start, span.end
    ))
}
fn declaration_error(db: &dyn Db, key: AstNodeKey, message: impl std::fmt::Display) -> SemanticError {
    let span = db
        .syntax_unit(key.unit)
        .filter(|syntax| syntax.accepts_key(db, key))
        .and_then(|syntax| syntax.syntax_index(db).metadata_for(key.generation, key.node).and_then(|m| m.span));
    match span {
        Some(span) => spanned(&span, message),
        None => invalid(&message.to_string()),
    }
}
/// Validate a Rust owner path: `::`-separated ASCII identifiers, first segment `implementation`,
/// no crate/relative roots, keywords, raw identifiers or generic arguments.
pub fn validate_rust_owner_path(text: &str) -> Result<(), SemanticError> {
    let segments = text.split("::").collect::<Vec<_>>();
    if text.len() > 4096 || segments.len() > 64 {
        return Err(invalid("Rust owner path exceeds bounded input"));
    }
    if segments.len() < 2 || segments[0] != "implementation" {
        return Err(invalid(&format!("Rust owner path `{text}` must start with `implementation::`")));
    }
    for segment in &segments[1..] {
        let mut chars = segment.chars();
        let identifier = matches!(chars.next(), Some(c) if c == '_' || c.is_ascii_alphabetic())
            && chars.all(|c| c == '_' || c.is_ascii_alphanumeric());
        if !identifier || RUST_OWNER_RESERVED_SEGMENTS.contains(segment) {
            return Err(invalid(&format!("Rust owner path `{text}` has invalid segment `{segment}`")));
        }
    }
    Ok(())
}
fn bool_attribute(attribute: &Attribute, name: &str) -> Result<Option<bool>, SemanticError> {
    let Some(arg) = attribute.arguments.iter().find(|a| a.node.name.node.name == name) else { return Ok(None) };
    let Expression::Literal(literal) = &arg.node.value.node else {
        return Err(invalid(&format!("RustOwner {name} must be a bool literal")));
    };
    let Literal::Bool(value) = literal.node.literal.node else {
        return Err(invalid(&format!("RustOwner {name} must be a bool literal")));
    };
    Ok(Some(value))
}
/// Read the at most one `RustOwner` attribute of a declaration for its closed placement.
fn rust_owner_attribute(
    attributes: &[Spanned<Attribute>],
    placement: RustOwnerPlacement,
) -> Result<Option<RustOwnerArguments>, SemanticError> {
    let mut markers = attributes.iter().filter(|a| a.node.name.node.name == RUST_OWNER);
    let Some(marker) = markers.next() else { return Ok(None) };
    if let Some(duplicate) = markers.next() {
        return Err(spanned(&duplicate.span, "duplicate RustOwner attribute"));
    }
    let attribute = &marker.node;
    if placement == RustOwnerPlacement::HandleType
        && attribute.arguments.iter().any(|a| a.node.name.node.name == "Fallible")
    {
        return Err(spanned(&marker.span, "RustOwner Fallible is not allowed on a GlueHandle type"));
    }
    let allowed: &[&str] = match placement {
        RustOwnerPlacement::HandleType => &["Path"],
        RustOwnerPlacement::ExternMethod => &["Path", "Fallible"],
    };
    closed_arguments(attribute, allowed)
        .map_err(|_| spanned(&marker.span, "unknown or repeated RustOwner argument"))?;
    let path = text_attribute(attribute, "Path").map_err(|error| spanned(&marker.span, error))?;
    if let Some(path) = &path {
        validate_rust_owner_path(path).map_err(|error| spanned(&marker.span, error))?;
    }
    let fallible = bool_attribute(attribute, "Fallible").map_err(|error| spanned(&marker.span, error))?;
    if placement == RustOwnerPlacement::HandleType && path.is_none() {
        return Err(spanned(&marker.span, "RustOwner on a GlueHandle type requires Path"));
    }
    Ok(Some(RustOwnerArguments { path, fallible }))
}
fn logical_type(
    db: &dyn Db,
    key: AstNodeKey,
    ty: &Type,
    generics: &[String],
    depth: usize,
    remaining: &mut usize,
) -> Result<GlueLogicalType, SemanticError> {
    if depth > 64 || *remaining == 0 {
        return Err(invalid("Glue type closure exceeds depth64/nodes1024"));
    }
    *remaining -= 1;
    match ty {
        Type::Primitive(value) => match value.node {
            PrimitiveType::Pointer | PrimitiveType::Never => {
                Err(invalid("raw pointer and never are unsupported Glue signatures"))
            }
            primitive => Ok(GlueLogicalType::Primitive(primitive)),
        },
        Type::Array(element) => {
            Ok(GlueLogicalType::Array(Box::new(logical_type(db, key, &element.node, generics, depth + 1, remaining)?)))
        }
        Type::Complex(path) => {
            if path.node.segments.len() == 1 && path.node.segments[0].node.type_args.is_empty() {
                let name = &path.node.segments[0].node.name.node.name;
                if let Some(position) = generics.iter().position(|g| g == name) {
                    return Ok(GlueLogicalType::Generic {
                        declaration: key,
                        position: position as u32,
                        name: name.clone(),
                    });
                }
            }
            let declaration =
                resolve_type_declaration(db, key, &path.node).ok_or_else(|| invalid("unresolved Glue nominal type"))?;
            let syntax =
                db.syntax_unit(declaration.unit).ok_or_else(|| invalid("unregistered Glue handle declaration"))?;
            if !syntax.accepts_key(db, declaration) {
                return Err(invalid("stale Glue handle declaration"));
            }
            let definition = syntax
                .syntax_index(db)
                .node_at(syntax.expanded_program(db), declaration.node)
                .and_then(|node| node.of::<TypeDefinition>())
                .ok_or_else(|| invalid("Glue nominal is not a type declaration"))?;
            if definition.attributes.len() > 256 || definition.generics.len() > 256 {
                return Err(invalid("Glue brand declaration metadata bound exceeded"));
            }
            if definition.attributes.iter().filter(|a| a.node.name.node.name == "GlueHandle").count() != 1 {
                return Err(invalid("nominal Glue types require exactly one validated GlueHandle brand"));
            }
            let marker = definition
                .attributes
                .iter()
                .find(|a| a.node.name.node.name == "GlueHandle")
                .ok_or_else(|| invalid("nominal Glue types require validated GlueHandle branding"))?;
            closed_arguments(&marker.node, &["Library", "Nullable"])?;
            let library = text_attribute(&marker.node, "Library")?
                .ok_or_else(|| invalid("GlueHandle requires Library metadata"))?;
            let nullable = if let Some(arg) = marker.node.arguments.iter().find(|a| a.node.name.node.name == "Nullable")
            {
                let Expression::Literal(literal) = &arg.node.value.node else {
                    return Err(invalid("GlueHandle Nullable must be bool"));
                };
                let Literal::Bool(value) = literal.node.literal.node else {
                    return Err(invalid("GlueHandle Nullable must be bool"));
                };
                value
            } else {
                false
            };
            // Branding is a declaration fact, never a primitive u64/name heuristic. Actual owner
            // token validation remains mandatory in the canonical runtime boundary.
            if definition.fields.len() != 1
                || !matches!(&definition.fields[0].node.ty.node,Type::Primitive(p) if p.node==PrimitiveType::U64)
            {
                return Err(invalid("GlueHandle requires exactly one u64 token field"));
            }
            let count = path.node.segments.iter().map(|s| s.node.type_args.len()).sum::<usize>();
            if count > *remaining {
                return Err(invalid("Glue generic argument closure exceeds nodes1024"));
            }
            let arguments = path
                .node
                .segments
                .iter()
                .flat_map(|s| s.node.type_args.iter())
                .map(|arg| logical_type(db, key, &arg.node, generics, depth + 1, remaining))
                .collect::<Result<Vec<_>, _>>()?;
            if definition.generics.len() != arguments.len() {
                return Err(invalid("GlueHandle generic arity differs from declaration"));
            }
            let rust_owner = rust_owner_attribute(&definition.attributes, RustOwnerPlacement::HandleType)?
                .and_then(|owner| owner.path)
                .map(Arc::<str>::from);
            Ok(GlueLogicalType::Handle(GlueHandleBrand {
                declaration,
                qualified_identity: calls::stable_declaration_identity(db, declaration)
                    .ok_or_else(|| invalid("missing canonical Glue handle identity"))?,
                library,
                nullable,
                arguments: arguments.into(),
                rust_owner,
            }))
        }
        Type::Associated { .. } | Type::This | Type::Function { .. } => {
            Err(invalid("unclosed associated, receiver, or callback Glue type"))
        }
    }
}
fn signature(
    db: &dyn Db,
    key: AstNodeKey,
    parameters: &[Spanned<Parameter>],
    result: Option<&Spanned<Type>>,
    generics: &[String],
) -> Result<(Arc<[GlueParameterFact]>, GlueLogicalType), SemanticError> {
    if parameters.len() > 256 {
        return Err(invalid("Glue parameter bound exceeded"));
    }
    let mut remaining = 1024;
    let parameters = parameters
        .iter()
        .map(|p| {
            if p.node.name.node.name.len() > 256 {
                return Err(invalid("Glue parameter name bound exceeded"));
            }
            if p.node.bulk {
                return Err(invalid("bulk parameters are not supported by the Glue profile"));
            }
            let ty = logical_type(db, key, &p.node.ty.node, generics, 0, &mut remaining)?;
            if ty == GlueLogicalType::Primitive(PrimitiveType::Unit) {
                return Err(invalid("Glue unit is return-only"));
            }
            Ok(GlueParameterFact { name: p.node.name.node.name.clone(), ty })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let result = result
        .map(|t| logical_type(db, key, &t.node, generics, 0, &mut remaining))
        .transpose()?
        .unwrap_or(GlueLogicalType::Primitive(PrimitiveType::Unit));
    Ok((parameters.into(), result))
}
#[salsa::tracked(persist)]
fn glue_binding_tracked(db: &dyn Db, syntax: SyntaxUnitInput, key: AstNodeKey) -> SemanticQueryResult<GlueBindingFact> {
    syntax_facts::with_node(db, syntax, key, |_program, _index, node| {
        if let Some(function) = node.of::<FunctionDefinition>() {
            let marker = function.attributes.iter().find(|a| a.node.name.node.name == "Export")?;
            let markers = function.attributes.iter().filter(|a| a.node.name.node.name == "Export").count();
            return Some((|| {
                if markers != 1 || function.attributes.len() > 256 {
                    return Err(invalid("duplicate Export attributes"));
                }
                if let Some(owner) = function.attributes.iter().find(|a| a.node.name.node.name == RUST_OWNER) {
                    return Err(spanned(&owner.span, "RustOwner is not allowed on an Export function"));
                }
                closed_arguments(&marker.node, &["Abi", "Symbol"])?;
                if text_attribute(&marker.node, "Abi")?.as_deref() != Some("C") {
                    return Err(invalid("Rust Glue exports require explicit C ABI"));
                }
                let decoded =
                    text_attribute(&marker.node, "Symbol")?.ok_or_else(|| invalid("missing Glue export symbol"))?;
                let symbol = queries::item_export_symbol(db, key)?
                    .ok_or_else(|| invalid("invalid canonical Glue export symbol"))?
                    .0
                    .to_string();
                if symbol != decoded {
                    return Err(invalid("canonical export metadata decoding mismatch"));
                }
                if function.generics.len() > 256 {
                    return Err(invalid("Glue generic parameter bound exceeded"));
                }
                if function.generics.iter().any(|g| g.node.name.len() > 256) {
                    return Err(invalid("Glue generic name bound exceeded"));
                }
                let generics = function.generics.iter().map(|g| g.node.name.clone()).collect::<Vec<_>>();
                if generics.iter().collect::<std::collections::HashSet<_>>().len() != generics.len() {
                    return Err(invalid("duplicate Glue generic declaration"));
                }
                let (parameters, result) =
                    signature(db, key, &function.parameters, function.return_type.as_ref(), &generics)?;
                Ok(GlueBindingFact {
                    declaration: key,
                    direction: GlueDirection::Export,
                    symbol,
                    library: None,
                    generics: generics.into(),
                    parameters,
                    result,
                    rust_owner: None,
                })
            })());
        }
        let method = node.of::<ContractMethodSignature>()?;
        let import = calls::extern_contract_import_for_declaration(db, key)?;
        let (symbol, abi, library) = (import.symbol, import.abi, import.library);
        Some((|| {
            let index = syntax.syntax_index(db);
            let mut parent = index.metadata_for(key.generation, key.node).and_then(|m| m.parent);
            let mut contract = None;
            while let Some(id) = parent {
                if let Some(value) = index
                    .node_at(syntax.expanded_program(db), id)
                    .and_then(|n| n.of::<beskid_analysis::syntax::ContractDefinition>())
                {
                    contract = Some(value);
                    break;
                }
                parent = index.metadata_for(key.generation, id).and_then(|m| m.parent);
            }
            let contract = contract.ok_or_else(|| invalid("missing canonical Extern owner"))?;
            if contract.attributes.len() > 256
                || contract.attributes.iter().filter(|a| a.node.name.node.name == "Extern").count() != 1
            {
                return Err(invalid("duplicate or excessive Extern owner metadata"));
            }
            let marker = contract
                .attributes
                .iter()
                .find(|a| a.node.name.node.name == "Extern")
                .ok_or_else(|| invalid("missing Extern owner metadata"))?;
            closed_arguments(&marker.node, &["Abi", "Library"])?;
            if text_attribute(&marker.node, "Abi")?.as_deref() != abi.as_deref()
                || text_attribute(&marker.node, "Library")? != library
            {
                return Err(invalid("canonical import metadata decoding mismatch"));
            }
            if abi.as_deref() != Some("C") {
                return Err(invalid("Rust Glue imports require explicit C ABI"));
            }
            let library = library
                .filter(|s| !s.is_empty() && !s.contains('\0'))
                .ok_or_else(|| invalid("Rust Glue import requires library"))?;
            if method.attributes.len() > 256 {
                return Err(invalid("Extern method metadata bound exceeded"));
            }
            let declared = rust_owner_attribute(&method.attributes, RustOwnerPlacement::ExternMethod)?;
            let rust_owner = GlueRustOwnerCallable {
                path: declared
                    .as_ref()
                    .and_then(|owner| owner.path.clone())
                    .unwrap_or_else(|| format!("implementation::{symbol}"))
                    .into(),
                fallible: declared.as_ref().and_then(|owner| owner.fallible).unwrap_or(false),
                declared: declared.is_some(),
            };
            let (parameters, result) = signature(db, key, &method.parameters, method.return_type.as_ref(), &[])?;
            Ok(GlueBindingFact {
                declaration: key,
                direction: GlueDirection::Import,
                symbol,
                library: Some(library),
                generics: Arc::from([]),
                parameters,
                result,
                rust_owner: Some(rust_owner),
            })
        })())
    })?
    .transpose()
}
pub fn glue_binding(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<GlueBindingFact> {
    queries::with_registered_syntax(db, key, glue_binding_tracked)
}

/// Project a branded nominal from the exact current binding type syntax. The
/// returned graph is privately constructed by the shared semantic shape issuer;
/// declaration names or caller DTOs cannot manufacture it.
pub fn glue_handle_shape(
    db: &dyn Db,
    key: AstNodeKey,
    position: Option<usize>,
) -> SemanticQueryResult<super::dynamic_pack::DynamicPackingShape> {
    let Some(fact) = glue_binding(db, key)? else { return Ok(None) };
    let ty = match position {
        Some(index) => fact.parameters.get(index).map(|parameter| &parameter.ty),
        None => Some(&fact.result),
    };
    if !matches!(ty, Some(GlueLogicalType::Handle(_))) {
        return Ok(None);
    }
    let syntax = db
        .syntax_unit(key.unit)
        .filter(|syntax| syntax.accepts_key(db, key))
        .ok_or_else(|| invalid("opaque shape binding is not current"))?;
    let node = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), key.node)
        .ok_or_else(|| invalid("opaque shape binding node absent"))?;
    let type_syntax = if let Some(function) = node.of::<FunctionDefinition>() {
        match position {
            Some(index) => function.parameters.get(index).map(|parameter| &parameter.node.ty),
            None => function.return_type.as_ref(),
        }
    } else if let Some(method) = node.of::<ContractMethodSignature>() {
        match position {
            Some(index) => method.parameters.get(index).map(|parameter| &parameter.node.ty),
            None => method.return_type.as_ref(),
        }
    } else {
        None
    }
    .ok_or_else(|| invalid("opaque binding type syntax absent"))?;
    let identity = calls::generic_source_type_identity(db, key, &type_syntax.node)?;
    Ok(Some(super::dynamic_pack::project_source_shape(db, key, &identity)?))
}

/// One explicit `RustOwner` placement in a source unit.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RustOwnerDeclaration {
    /// The `GlueHandle` type or `Extern` contract method that carries the attribute.
    pub declaration: AstNodeKey,
    pub placement: RustOwnerPlacement,
    /// The `GlueHandle` or `Extern` `Library`. The caller checks it against the manifest
    /// `glue "<library>" { backend = rust ... }` blocks: a library without one is rejected.
    pub library: String,
    /// Declared or default (`implementation::<symbol>`) Rust path.
    pub path: Arc<str>,
    pub fallible: bool,
}

/// Validate every `RustOwner` attribute of one registered source unit and list its placements.
///
/// Rejects, with the attribute span, a `RustOwner` attribute on any declaration other than a
/// `GlueHandle` type or an `Extern` contract method, duplicates, unknown or repeated arguments,
/// `Fallible` on a type, a type without `Path`, and invalid paths. A unit without the attribute
/// yields an empty list. Manifest data is not visible here: the caller rejects every listed
/// `library` that has no manifest Rust glue block.
pub fn rust_owner_declarations(db: &dyn Db, unit: SourceUnitId) -> Result<Arc<[RustOwnerDeclaration]>, SemanticError> {
    let syntax = db.syntax_unit(unit).ok_or_else(|| invalid("RustOwner source unit is not registered"))?;
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let generation = index.generation();
    let mut owners = Vec::new();
    let mut declarations = Vec::new();
    for id in index.ids_of_kind(NodeKind::Attribute) {
        let attribute = index
            .node_at(program, id)
            .and_then(|node| node.of::<Attribute>())
            .ok_or_else(|| invalid("indexed attribute node absent"))?;
        if attribute.name.node.name != RUST_OWNER {
            continue;
        }
        let metadata = index.metadata_for(generation, id).ok_or_else(|| invalid("RustOwner metadata absent"))?;
        let misplaced = |message: &str| match metadata.span {
            Some(span) => spanned(&span, message),
            None => invalid(message),
        };
        let parent = metadata.parent.ok_or_else(|| misplaced("RustOwner is not attached to a declaration"))?;
        if owners.contains(&parent) {
            continue;
        }
        owners.push(parent);
        let declaration = AstNodeKey { unit, generation, node: parent };
        let node = index
            .node_at(program, parent)
            .ok_or_else(|| misplaced("RustOwner declaration node absent"))?;
        if let Some(definition) = node.of::<TypeDefinition>() {
            let mut handles = definition.attributes.iter().filter(|a| a.node.name.node.name == "GlueHandle");
            let (Some(handle), None) = (handles.next(), handles.next()) else {
                return Err(misplaced("RustOwner is allowed only on a GlueHandle type or an Extern contract method"));
            };
            let library = text_attribute(&handle.node, "Library")
                .map_err(|error| spanned(&handle.span, error))?
                .ok_or_else(|| spanned(&handle.span, "GlueHandle requires Library metadata"))?;
            let owner = rust_owner_attribute(&definition.attributes, RustOwnerPlacement::HandleType)?
                .ok_or_else(|| misplaced("RustOwner attribute absent"))?;
            let path = owner.path.ok_or_else(|| misplaced("RustOwner on a GlueHandle type requires Path"))?;
            declarations.push(RustOwnerDeclaration {
                declaration,
                placement: RustOwnerPlacement::HandleType,
                library,
                path: path.into(),
                fallible: false,
            });
        } else if let Some(method) = node.of::<ContractMethodSignature>() {
            let import = calls::extern_contract_import_for_declaration(db, declaration)
                .ok_or_else(|| misplaced("RustOwner is allowed only on a GlueHandle type or an Extern contract method"))?;
            if import.availability_query {
                return Err(misplaced("RustOwner is not allowed on the compiler-supplied Available query"));
            }
            let (symbol, library) = (import.symbol, import.library);
            let library = library
                .filter(|library| !library.is_empty())
                .ok_or_else(|| misplaced("RustOwner Extern contract requires Library metadata"))?;
            let owner = rust_owner_attribute(&method.attributes, RustOwnerPlacement::ExternMethod)?
                .ok_or_else(|| misplaced("RustOwner attribute absent"))?;
            let path = owner.path.unwrap_or_else(|| format!("implementation::{symbol}"));
            validate_rust_owner_path(&path).map_err(|error| misplaced(&error.to_string()))?;
            declarations.push(RustOwnerDeclaration {
                declaration,
                placement: RustOwnerPlacement::ExternMethod,
                library,
                path: path.into(),
                fallible: owner.fallible.unwrap_or(false),
            });
        } else {
            return Err(misplaced("RustOwner is allowed only on a GlueHandle type or an Extern contract method"));
        }
    }
    Ok(declarations.into())
}

/// Compiler-issued digests of one canonical Glue binding row, taken from the Glue artifact that
/// the same generation produced. It carries no mapping data: paths and fallibility come only from
/// the `RustOwner` source facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlueOwnerBindingDigest {
    pub binding: AstNodeKey,
    pub symbol: String,
    pub identity_sha256: String,
    /// Opaque brand digest per handle position: `Some(index)` for a parameter, `None` for the result.
    pub opaque: Vec<(Option<usize>, String)>,
}
/// One `beskid_aot` `RustOwnerType` row.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RustOwnerTypeRow {
    pub brand_sha256: String,
    pub rust_type_path: String,
}
/// One `beskid_aot` `RustOwnerCallable` row.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RustOwnerCallableRow {
    pub binding_identity_sha256: String,
    pub rust_callable_path: String,
    pub fallible: bool,
}
/// The complete type and callable inputs of one Rust owner build, derived from source facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustOwnerTables {
    pub library: String,
    pub types: Vec<RustOwnerTypeRow>,
    pub callables: Vec<RustOwnerCallableRow>,
}
fn sha256_text(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
/// Form the Rust owner type and callable tables of `library` from `RustOwner` facts.
///
/// Every row must be an import of `library`. Each handle position of the canonical signature
/// must have exactly one digest, and each handle must belong to `library` and declare
/// `RustOwner(Path:..)`. Callables take the declared or default path and fallibility. Each source
/// unit involved also passes [`rust_owner_declarations`], so a misplaced attribute anywhere in it
/// fails the build.
pub fn rust_owner_tables(
    db: &dyn Db,
    library: &str,
    bindings: &[GlueOwnerBindingDigest],
) -> Result<RustOwnerTables, SemanticError> {
    if library.is_empty() || bindings.is_empty() || bindings.len() > 4096 {
        return Err(invalid("Rust owner tables require a library and 1..=4096 bindings"));
    }
    let mut units = Vec::new();
    let mut types = std::collections::BTreeMap::<String, (AstNodeKey, Arc<str>)>::new();
    let mut callables = std::collections::BTreeMap::<String, RustOwnerCallableRow>::new();
    for row in bindings {
        let fact = glue_binding(db, row.binding)?
            .ok_or_else(|| invalid("Rust owner binding has no canonical Glue fact"))?;
        if fact.direction != GlueDirection::Import
            || fact.library.as_deref() != Some(library)
            || fact.symbol != row.symbol
        {
            return Err(declaration_error(
                db,
                row.binding,
                format!("Rust owner row `{}` is not an import of `{library}`", row.symbol),
            ));
        }
        if !sha256_text(&row.identity_sha256) {
            return Err(invalid("Rust owner binding identity is not a sha256 digest"));
        }
        let owner = fact.rust_owner.as_ref().ok_or_else(|| invalid("Rust owner import mapping absent"))?;
        validate_rust_owner_path(&owner.path).map_err(|error| declaration_error(db, row.binding, error))?;
        let callable = RustOwnerCallableRow {
            binding_identity_sha256: row.identity_sha256.clone(),
            rust_callable_path: owner.path.to_string(),
            fallible: owner.fallible,
        };
        if callables.insert(row.identity_sha256.clone(), callable).is_some() {
            return Err(invalid("duplicate Rust owner binding identity"));
        }
        if !units.contains(&row.binding.unit) {
            units.push(row.binding.unit);
        }
        let handles = fact
            .parameters
            .iter()
            .enumerate()
            .map(|(index, parameter)| (Some(index), &parameter.ty))
            .chain(std::iter::once((None, &fact.result)))
            .filter_map(|(position, ty)| match ty {
                GlueLogicalType::Handle(brand) => Some((position, brand)),
                _ => None,
            })
            .collect::<Vec<_>>();
        if row.opaque.len() != handles.len() {
            return Err(invalid("Rust owner opaque digests differ from the canonical handle positions"));
        }
        for (position, brand) in handles {
            let mut digests = row.opaque.iter().filter(|(supplied, _)| *supplied == position);
            let (Some((_, digest)), None) = (digests.next(), digests.next()) else {
                return Err(invalid("Rust owner opaque digests differ from the canonical handle positions"));
            };
            if !sha256_text(digest) {
                return Err(invalid("Rust owner brand is not a sha256 digest"));
            }
            if brand.library != library {
                return Err(declaration_error(
                    db,
                    brand.declaration,
                    format!("GlueHandle library `{}` is not Rust owner `{library}`", brand.library),
                ));
            }
            let path = brand.rust_owner.clone().ok_or_else(|| {
                declaration_error(db, brand.declaration, "GlueHandle type of a Rust glue owner requires RustOwner(Path:..)")
            })?;
            match types.entry(digest.clone()) {
                std::collections::btree_map::Entry::Occupied(entry) => {
                    if entry.get().0 != brand.declaration || entry.get().1 != path {
                        return Err(invalid("one Rust owner brand maps to two declarations"));
                    }
                }
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert((brand.declaration, path));
                }
            }
            if !units.contains(&brand.declaration.unit) {
                units.push(brand.declaration.unit);
            }
        }
    }
    for unit in units {
        rust_owner_declarations(db, unit)?;
    }
    Ok(RustOwnerTables {
        library: library.to_owned(),
        types: types
            .into_iter()
            .map(|(brand_sha256, (_, path))| RustOwnerTypeRow { brand_sha256, rust_type_path: path.to_string() })
            .collect(),
        callables: callables.into_values().collect(),
    })
}
