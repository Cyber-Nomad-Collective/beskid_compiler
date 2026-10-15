//! Canonical generation-bound logical Glue bindings. Physical transport is emitted later.
use super::*;
use beskid_analysis::syntax::{
    Attribute, ContractMethodSignature, Expression, FunctionDefinition, Literal, Parameter, PrimitiveType, SpanInfo,
    Spanned, Type, TypeDefinition,
};
use beskid_analysis::syntax_query::NodeKind;
use beskid_abi::interop::{InteropSignature, ScalarDomain, ScalarShape, TypeShape};

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
/// One field of a `[GlueRecord]` in declaration order.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GlueRecordField {
    pub name: String,
    pub ty: PrimitiveType,
}
/// A `[GlueRecord]` declaration crossing by value as a C struct.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GlueRecordBrand {
    pub declaration: AstNodeKey,
    pub qualified_identity: Arc<str>,
    /// `[GlueRecord(Library:..)]`; required with `RustOwner`.
    pub library: Option<String>,
    pub fields: Arc<[GlueRecordField]>,
    /// Declared `[RustOwner(Path:..)]` Rust type path.
    pub rust_owner: Option<Arc<str>>,
}
/// A fieldless `[GlueEnum]` declaration crossing as its `u32` source-order variant index.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GlueEnumBrand {
    pub declaration: AstNodeKey,
    pub qualified_identity: Arc<str>,
    /// `[GlueEnum(Library:..)]`; required with `RustOwner`.
    pub library: Option<String>,
    pub variants: Arc<[String]>,
    /// Declared `[RustOwner(Path:..)]` Rust type path.
    pub rust_owner: Option<Arc<str>>,
}
/// A call-scoped callback: an import parameter of function type whose members are scalar forms.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GlueCallbackSignature {
    pub parameters: Arc<[GlueLogicalType]>,
    pub result: Box<GlueLogicalType>,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum GlueLogicalType {
    Primitive(PrimitiveType),
    Array(Box<GlueLogicalType>),
    Generic { declaration: AstNodeKey, position: u32, name: String },
    Handle(GlueHandleBrand),
    Record(GlueRecordBrand),
    Enum(GlueEnumBrand),
    Callback(GlueCallbackSignature),
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
/// Rejection text of a misplaced `RustOwner`; it names every closed placement.
const RUST_OWNER_PLACEMENTS: &str =
    "RustOwner is allowed only on a GlueHandle, GlueRecord or GlueEnum type or an Extern contract method";
/// Segments that cannot appear in a Rust owner path. A path resolves only inside the generated
/// `implementation` module, so crate roots, relative roots and Rust keywords are closed out.
const RUST_OWNER_RESERVED_SEGMENTS: &[&str] = &[
    "crate", "self", "Self", "super", "std", "core", "alloc", "as", "async", "await", "break", "const", "continue",
    "dyn", "else", "enum", "extern", "false", "fn", "for", "gen", "if", "impl", "in", "let", "loop", "match", "mod",
    "move", "mut", "pub", "ref", "return", "static", "struct", "trait", "true", "try", "type", "unsafe", "use",
    "where", "while", "abstract", "become", "box", "do", "final", "macro", "override", "priv", "typeof", "unsized",
    "virtual", "yield", "_",
];
/// The closed `RustOwner` placements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum RustOwnerPlacement {
    /// `[RustOwner(Path:..)]` on a `GlueHandle` type. `Path` is required, `Fallible` is rejected.
    HandleType,
    /// `[RustOwner(Path:.., Fallible:..)]` on an `Extern` contract method. Both are optional.
    ExternMethod,
    /// `[RustOwner(Path:..)]` on a `GlueRecord` type. `Path` is required, `Fallible` is rejected.
    RecordType,
    /// `[RustOwner(Path:..)]` on a `GlueEnum` type. `Path` is required, `Fallible` is rejected.
    EnumType,
}
impl RustOwnerPlacement {
    fn type_marker(self) -> Option<&'static str> {
        match self {
            Self::HandleType => Some("GlueHandle"),
            Self::RecordType => Some("GlueRecord"),
            Self::EnumType => Some("GlueEnum"),
            Self::ExternMethod => None,
        }
    }
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
    if let Some(type_marker) = placement.type_marker()
        && attribute.arguments.iter().any(|a| a.node.name.node.name == "Fallible")
    {
        return Err(spanned(&marker.span, format!("RustOwner Fallible is not allowed on a {type_marker} type")));
    }
    let allowed: &[&str] = match placement {
        RustOwnerPlacement::ExternMethod => &["Path", "Fallible"],
        _ => &["Path"],
    };
    closed_arguments(attribute, allowed)
        .map_err(|_| spanned(&marker.span, "unknown or repeated RustOwner argument"))?;
    let path = text_attribute(attribute, "Path").map_err(|error| spanned(&marker.span, error))?;
    if let Some(path) = &path {
        validate_rust_owner_path(path).map_err(|error| spanned(&marker.span, error))?;
    }
    let fallible = bool_attribute(attribute, "Fallible").map_err(|error| spanned(&marker.span, error))?;
    if let Some(type_marker) = placement.type_marker()
        && path.is_none()
    {
        return Err(spanned(&marker.span, format!("RustOwner on a {type_marker} type requires Path")));
    }
    Ok(Some(RustOwnerArguments { path, fallible }))
}
/// Where a Glue type appears. Callbacks are admitted only as import parameters, and the members
/// of a callback signature only as scalar forms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GluePosition {
    ImportParameter,
    Value,
    CallbackMember,
}
const GLUE_HANDLE: &str = "GlueHandle";
const GLUE_RECORD: &str = "GlueRecord";
const GLUE_ENUM: &str = "GlueEnum";
/// Primitive fields a `[GlueRecord]` admits: the fixed-width scalars, floats, Bool, `char` and
/// `word`. Views, handles, nested records, enums, arrays and function types are not record fields.
fn record_field_primitive(primitive: PrimitiveType) -> bool {
    matches!(
        primitive,
        PrimitiveType::I8
            | PrimitiveType::I16
            | PrimitiveType::I32
            | PrimitiveType::I64
            | PrimitiveType::U8
            | PrimitiveType::U16
            | PrimitiveType::U32
            | PrimitiveType::U64
            | PrimitiveType::F32
            | PrimitiveType::F64
            | PrimitiveType::Bool
            | PrimitiveType::Char
            | PrimitiveType::Word
    )
}
/// `Library` of a `GlueRecord`/`GlueEnum` marker, which is optional; the marker takes no other
/// argument.
fn value_marker_library(attribute: &Attribute) -> Result<Option<String>, SemanticError> {
    closed_arguments(attribute, &["Library"])?;
    text_attribute(attribute, "Library")
}
fn logical_type(
    db: &dyn Db,
    key: AstNodeKey,
    ty: &Type,
    generics: &[String],
    depth: usize,
    remaining: &mut usize,
    position: GluePosition,
) -> Result<GlueLogicalType, SemanticError> {
    if depth > 64 || *remaining == 0 {
        return Err(invalid("Glue type closure exceeds depth64/nodes1024"));
    }
    *remaining -= 1;
    let callback_member = position == GluePosition::CallbackMember;
    match ty {
        Type::Primitive(value) => match value.node {
            PrimitiveType::Pointer | PrimitiveType::Never => {
                Err(invalid("raw pointer and never are unsupported Glue signatures"))
            }
            PrimitiveType::String if callback_member => {
                Err(invalid("Glue callback parameters and results are scalar forms; string is a view"))
            }
            primitive => Ok(GlueLogicalType::Primitive(primitive)),
        },
        Type::Array(_) if callback_member => {
            Err(invalid("Glue callback parameters and results are scalar forms; arrays are views"))
        }
        Type::Array(element) => Ok(GlueLogicalType::Array(Box::new(logical_type(
            db,
            key,
            &element.node,
            generics,
            depth + 1,
            remaining,
            GluePosition::Value,
        )?))),
        Type::Complex(path) => {
            if path.node.segments.len() == 1 && path.node.segments[0].node.type_args.is_empty() {
                let name = &path.node.segments[0].node.name.node.name;
                if let Some(position) = generics.iter().position(|g| g == name) {
                    if callback_member {
                        return Err(invalid("Glue callback members cannot be generic positions"));
                    }
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
                db.syntax_unit(declaration.unit).ok_or_else(|| invalid("unregistered Glue nominal declaration"))?;
            if !syntax.accepts_key(db, declaration) {
                return Err(invalid("stale Glue nominal declaration"));
            }
            let node = syntax
                .syntax_index(db)
                .node_at(syntax.expanded_program(db), declaration.node)
                .ok_or_else(|| invalid("Glue nominal declaration node absent"))?;
            let identity = || {
                calls::stable_declaration_identity(db, declaration)
                    .ok_or_else(|| invalid("missing canonical Glue nominal identity"))
            };
            if let Some(definition) = node.of::<beskid_analysis::syntax::EnumDefinition>() {
                return glue_enum(definition, declaration, path, identity()?).map(GlueLogicalType::Enum);
            }
            let definition =
                node.of::<TypeDefinition>().ok_or_else(|| invalid("Glue nominal is not a type declaration"))?;
            if definition.attributes.len() > 256 || definition.generics.len() > 256 {
                return Err(invalid("Glue brand declaration metadata bound exceeded"));
            }
            let handles = definition.attributes.iter().filter(|a| a.node.name.node.name == GLUE_HANDLE).count();
            let records = definition.attributes.iter().filter(|a| a.node.name.node.name == GLUE_RECORD).count();
            if handles + records != 1 {
                return Err(invalid(
                    "nominal Glue types require exactly one validated GlueHandle brand or GlueRecord marker",
                ));
            }
            if records == 1 {
                if callback_member {
                    return Err(invalid("Glue callback parameters and results are scalar forms; records are not"));
                }
                return glue_record(definition, declaration, path, identity()?).map(GlueLogicalType::Record);
            }
            if callback_member {
                return Err(invalid("Glue callback parameters and results are scalar forms; handles are not"));
            }
            let marker = definition
                .attributes
                .iter()
                .find(|a| a.node.name.node.name == GLUE_HANDLE)
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
                .map(|arg| logical_type(db, key, &arg.node, generics, depth + 1, remaining, GluePosition::Value))
                .collect::<Result<Vec<_>, _>>()?;
            if definition.generics.len() != arguments.len() {
                return Err(invalid("GlueHandle generic arity differs from declaration"));
            }
            let rust_owner = rust_owner_attribute(&definition.attributes, RustOwnerPlacement::HandleType)?
                .and_then(|owner| owner.path)
                .map(Arc::<str>::from);
            Ok(GlueLogicalType::Handle(GlueHandleBrand {
                declaration,
                qualified_identity: identity()?,
                library,
                nullable,
                arguments: arguments.into(),
                rust_owner,
            }))
        }
        Type::Function { return_type, parameters } => {
            if position != GluePosition::ImportParameter {
                return Err(invalid(
                    "Glue callbacks are admitted only as import parameters; foreign function pointers are not Beskid values",
                ));
            }
            if parameters.len() > beskid_abi::interop::c_profile::C_PROFILE_CALLBACK_PARAMETER_LIMIT {
                return Err(invalid("Glue callbacks take at most 16 parameters"));
            }
            let parameters = parameters
                .iter()
                .map(|parameter| {
                    let ty = logical_type(
                        db,
                        key,
                        &parameter.node,
                        generics,
                        depth + 1,
                        remaining,
                        GluePosition::CallbackMember,
                    )?;
                    if ty == GlueLogicalType::Primitive(PrimitiveType::Unit) {
                        return Err(invalid("Glue unit is return-only"));
                    }
                    Ok(ty)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let result = logical_type(
                db,
                key,
                &return_type.node,
                generics,
                depth + 1,
                remaining,
                GluePosition::CallbackMember,
            )?;
            Ok(GlueLogicalType::Callback(GlueCallbackSignature { parameters: parameters.into(), result: Box::new(result) }))
        }
        Type::Associated { .. } | Type::This => Err(invalid("unclosed associated or receiver Glue type")),
    }
}
/// Admit a `[GlueRecord]` declaration: non-generic, 1..=64 value fields of admitted scalar
/// primitives, unique names, and an optional `Library` and `RustOwner(Path:..)`.
fn glue_record(
    definition: &TypeDefinition,
    declaration: AstNodeKey,
    path: &Spanned<beskid_analysis::syntax::Path>,
    qualified_identity: Arc<str>,
) -> Result<GlueRecordBrand, SemanticError> {
    let marker = definition
        .attributes
        .iter()
        .find(|a| a.node.name.node.name == GLUE_RECORD)
        .ok_or_else(|| invalid("GlueRecord marker absent"))?;
    let library = value_marker_library(&marker.node).map_err(|error| spanned(&marker.span, error))?;
    if !definition.generics.is_empty() || path.node.segments.iter().any(|segment| !segment.node.type_args.is_empty()) {
        return Err(spanned(&marker.span, "GlueRecord types are not generic"));
    }
    if definition.fields.is_empty() || definition.fields.len() > beskid_abi::interop::c_profile::C_PROFILE_RECORD_FIELD_LIMIT {
        return Err(spanned(&marker.span, "GlueRecord types have 1..=64 fields"));
    }
    let mut names = std::collections::HashSet::new();
    let fields = definition
        .fields
        .iter()
        .map(|field| {
            let name = field.node.name.node.name.clone();
            if name.len() > 256 || !names.insert(name.clone()) {
                return Err(spanned(&field.span, "GlueRecord field names are unique and at most 256 bytes"));
            }
            if field.node.kind != beskid_analysis::syntax::FieldKind::Value {
                return Err(spanned(&field.span, "GlueRecord fields are value fields"));
            }
            match &field.node.ty.node {
                Type::Primitive(primitive) if record_field_primitive(primitive.node) => {
                    Ok(GlueRecordField { name, ty: primitive.node })
                }
                _ => Err(spanned(
                    &field.span,
                    format!(
                        "GlueRecord field `{name}` must be i8..i64, u8..u64, f32, f64, bool, char or word"
                    ),
                )),
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    let rust_owner = rust_owner_attribute(&definition.attributes, RustOwnerPlacement::RecordType)?
        .and_then(|owner| owner.path)
        .map(Arc::<str>::from);
    if rust_owner.is_some() && library.is_none() {
        return Err(spanned(&marker.span, "RustOwner on a GlueRecord type requires GlueRecord Library"));
    }
    Ok(GlueRecordBrand { declaration, qualified_identity, library, fields: fields.into(), rust_owner })
}
/// Admit a `[GlueEnum]` declaration: non-generic, 1..=4096 variants, none with fields.
fn glue_enum(
    definition: &beskid_analysis::syntax::EnumDefinition,
    declaration: AstNodeKey,
    path: &Spanned<beskid_analysis::syntax::Path>,
    qualified_identity: Arc<str>,
) -> Result<GlueEnumBrand, SemanticError> {
    if definition.attributes.len() > 256 {
        return Err(invalid("Glue enum declaration metadata bound exceeded"));
    }
    let mut markers = definition.attributes.iter().filter(|a| a.node.name.node.name == GLUE_ENUM);
    let (Some(marker), None) = (markers.next(), markers.next()) else {
        return Err(invalid("Glue enums require exactly one GlueEnum marker"));
    };
    let library = value_marker_library(&marker.node).map_err(|error| spanned(&marker.span, error))?;
    if !definition.generics.is_empty() || path.node.segments.iter().any(|segment| !segment.node.type_args.is_empty()) {
        return Err(spanned(&marker.span, "GlueEnum types are not generic"));
    }
    if definition.variants.is_empty()
        || definition.variants.len() > beskid_abi::interop::c_profile::C_PROFILE_ENUMERATION_VARIANT_LIMIT
    {
        return Err(spanned(&marker.span, "GlueEnum types have 1..=4096 variants"));
    }
    let variants = definition
        .variants
        .iter()
        .map(|variant| {
            if !variant.node.fields.is_empty() {
                return Err(spanned(
                    &variant.span,
                    format!(
                        "GlueEnum variant `{}` has fields; tagged variants are not admitted",
                        variant.node.name.node.name
                    ),
                ));
            }
            Ok(variant.node.name.node.name.clone())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let rust_owner = rust_owner_attribute(&definition.attributes, RustOwnerPlacement::EnumType)?
        .and_then(|owner| owner.path)
        .map(Arc::<str>::from);
    if rust_owner.is_some() && library.is_none() {
        return Err(spanned(&marker.span, "RustOwner on a GlueEnum type requires GlueEnum Library"));
    }
    Ok(GlueEnumBrand { declaration, qualified_identity, library, variants: variants.into(), rust_owner })
}
fn signature(
    db: &dyn Db,
    key: AstNodeKey,
    parameters: &[Spanned<Parameter>],
    result: Option<&Spanned<Type>>,
    generics: &[String],
    direction: &GlueDirection,
) -> Result<(Arc<[GlueParameterFact]>, GlueLogicalType), SemanticError> {
    if parameters.len() > 256 {
        return Err(invalid("Glue parameter bound exceeded"));
    }
    let mut remaining = 1024;
    let parameter_position = match direction {
        GlueDirection::Import => GluePosition::ImportParameter,
        GlueDirection::Export => GluePosition::Value,
    };
    let parameters = parameters
        .iter()
        .map(|p| {
            if p.node.name.node.name.len() > 256 {
                return Err(invalid("Glue parameter name bound exceeded"));
            }
            if p.node.bulk {
                return Err(invalid("bulk parameters are not supported by the Glue profile"));
            }
            let ty = logical_type(db, key, &p.node.ty.node, generics, 0, &mut remaining, parameter_position)?;
            if ty == GlueLogicalType::Primitive(PrimitiveType::Unit) {
                return Err(invalid("Glue unit is return-only"));
            }
            Ok(GlueParameterFact { name: p.node.name.node.name.clone(), ty })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let result = result
        .map(|t| logical_type(db, key, &t.node, generics, 0, &mut remaining, GluePosition::Value))
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
                    signature(db, key, &function.parameters, function.return_type.as_ref(), &generics, &GlueDirection::Export)?;
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
            let (parameters, result) = signature(db, key, &method.parameters, method.return_type.as_ref(), &[], &GlueDirection::Import)?;
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
        let type_attributes = node
            .of::<TypeDefinition>()
            .map(|definition| &definition.attributes)
            .or_else(|| node.of::<beskid_analysis::syntax::EnumDefinition>().map(|definition| &definition.attributes));
        if let Some(attributes) = type_attributes {
            let mut markers = attributes.iter().filter(|a| {
                let name = a.node.name.node.name.as_str();
                if node.of::<TypeDefinition>().is_some() { name == GLUE_HANDLE || name == GLUE_RECORD } else { name == GLUE_ENUM }
            });
            let (Some(marker), None) = (markers.next(), markers.next()) else {
                return Err(misplaced(RUST_OWNER_PLACEMENTS));
            };
            let placement = match marker.node.name.node.name.as_str() {
                GLUE_HANDLE => RustOwnerPlacement::HandleType,
                GLUE_RECORD => RustOwnerPlacement::RecordType,
                _ => RustOwnerPlacement::EnumType,
            };
            let marker_name = &marker.node.name.node.name;
            let library = text_attribute(&marker.node, "Library")
                .map_err(|error| spanned(&marker.span, error))?
                .ok_or_else(|| spanned(&marker.span, format!("RustOwner on a {marker_name} type requires {marker_name} Library")))?;
            let owner = rust_owner_attribute(attributes, placement)?.ok_or_else(|| misplaced("RustOwner attribute absent"))?;
            let path = owner.path.ok_or_else(|| misplaced(&format!("RustOwner on a {marker_name} type requires Path")))?;
            declarations.push(RustOwnerDeclaration { declaration, placement, library, path: path.into(), fallible: false });
        } else if let Some(method) = node.of::<ContractMethodSignature>() {
            let import = calls::extern_contract_import_for_declaration(db, declaration)
                .ok_or_else(|| misplaced(RUST_OWNER_PLACEMENTS))?;
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
            return Err(misplaced(RUST_OWNER_PLACEMENTS));
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
        // Records and enums cross by value; the owner crate names their Rust types.
        let mut values = Vec::new();
        for ty in fact.parameters.iter().map(|parameter| &parameter.ty).chain(std::iter::once(&fact.result)) {
            match ty {
                GlueLogicalType::Record(record) => {
                    values.push((record.declaration, "GlueRecord", record.library.as_deref(), record.rust_owner.is_some()))
                }
                GlueLogicalType::Enum(enumeration) => values.push((
                    enumeration.declaration,
                    "GlueEnum",
                    enumeration.library.as_deref(),
                    enumeration.rust_owner.is_some(),
                )),
                GlueLogicalType::Callback(callback) => {
                    for member in callback.parameters.iter().chain(std::iter::once(callback.result.as_ref())) {
                        if let GlueLogicalType::Enum(enumeration) = member {
                            values.push((
                                enumeration.declaration,
                                "GlueEnum",
                                enumeration.library.as_deref(),
                                enumeration.rust_owner.is_some(),
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
        for (declaration, marker, value_library, owned) in values {
            if value_library != Some(library) {
                return Err(declaration_error(
                    db,
                    declaration,
                    format!("{marker} type used by Rust owner `{library}` requires {marker}(Library:\"{library}\")"),
                ));
            }
            if !owned {
                return Err(declaration_error(
                    db,
                    declaration,
                    format!("{marker} type of a Rust glue owner requires RustOwner(Path:..)"),
                ));
            }
            if !units.contains(&declaration.unit) {
                units.push(declaration.unit);
            }
        }
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

/// A Glue logical type with every declaration key replaced by its stable qualified identity:
/// the form a boundary shape can carry. [`glue_logical_form`] lifts a lowered shape back to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlueLogicalForm {
    Primitive(PrimitiveType),
    Bytes,
    Handle { identity: String, nullable: bool, arguments: Vec<GlueLogicalForm> },
    Record { identity: String, fields: Vec<(String, PrimitiveType)> },
    Enum { identity: String, variants: Vec<String> },
    Callback { parameters: Vec<GlueLogicalForm>, result: Box<GlueLogicalForm> },
}
impl GlueLogicalForm {
    /// The form of an admitted logical type. Generic positions and arrays of anything but `u8`
    /// are not admitted forms.
    pub fn of(ty: &GlueLogicalType) -> Result<Self, SemanticError> {
        Ok(match ty {
            GlueLogicalType::Primitive(primitive) => Self::Primitive(*primitive),
            GlueLogicalType::Array(element) if **element == GlueLogicalType::Primitive(PrimitiveType::U8) => Self::Bytes,
            GlueLogicalType::Array(_) => return Err(invalid("only u8 arrays are admitted Glue views")),
            GlueLogicalType::Generic { name, .. } => {
                return Err(invalid(&format!("generic Glue position `{name}` must be specialized before lowering")));
            }
            GlueLogicalType::Handle(brand) => Self::Handle {
                identity: brand.qualified_identity.to_string(),
                nullable: brand.nullable,
                arguments: brand.arguments.iter().map(Self::of).collect::<Result<_, _>>()?,
            },
            GlueLogicalType::Record(record) => Self::Record {
                identity: record.qualified_identity.to_string(),
                fields: record.fields.iter().map(|field| (field.name.clone(), field.ty)).collect(),
            },
            GlueLogicalType::Enum(enumeration) => Self::Enum {
                identity: enumeration.qualified_identity.to_string(),
                variants: enumeration.variants.to_vec(),
            },
            GlueLogicalType::Callback(callback) => Self::Callback {
                parameters: callback.parameters.iter().map(Self::of).collect::<Result<_, _>>()?,
                result: Box::new(Self::of(&callback.result)?),
            },
        })
    }
}
fn glue_scalar_shape(primitive: PrimitiveType) -> Option<ScalarShape> {
    use beskid_abi::abi_v5::AbiType;
    Some(match primitive {
        PrimitiveType::I8 => ScalarShape::raw(AbiType::I8),
        PrimitiveType::I16 => ScalarShape::raw(AbiType::I16),
        PrimitiveType::I32 => ScalarShape::raw(AbiType::I32),
        PrimitiveType::I64 => ScalarShape::raw(AbiType::I64),
        PrimitiveType::U8 => ScalarShape::raw(AbiType::U8),
        PrimitiveType::U16 => ScalarShape::raw(AbiType::U16),
        PrimitiveType::U32 => ScalarShape::raw(AbiType::U32),
        PrimitiveType::U64 => ScalarShape::raw(AbiType::U64),
        PrimitiveType::F32 => ScalarShape::raw(AbiType::F32),
        PrimitiveType::F64 => ScalarShape::raw(AbiType::F64),
        PrimitiveType::Bool => ScalarShape::new(AbiType::U8, ScalarDomain::Bool),
        PrimitiveType::Char => ScalarShape::new(AbiType::U32, ScalarDomain::UnicodeScalar),
        PrimitiveType::Word => ScalarShape::new(AbiType::USize, ScalarDomain::NativeWord),
        PrimitiveType::String | PrimitiveType::Unit | PrimitiveType::Pointer | PrimitiveType::Never => return None,
    })
}
fn glue_scalar_primitive(scalar: &ScalarShape) -> Option<PrimitiveType> {
    use beskid_abi::abi_v5::AbiType;
    Some(match (scalar.domain, scalar.abi_type) {
        (ScalarDomain::Raw, AbiType::I8) => PrimitiveType::I8,
        (ScalarDomain::Raw, AbiType::I16) => PrimitiveType::I16,
        (ScalarDomain::Raw, AbiType::I32) => PrimitiveType::I32,
        (ScalarDomain::Raw, AbiType::I64) => PrimitiveType::I64,
        (ScalarDomain::Raw, AbiType::U8) => PrimitiveType::U8,
        (ScalarDomain::Raw, AbiType::U16) => PrimitiveType::U16,
        (ScalarDomain::Raw, AbiType::U32) => PrimitiveType::U32,
        (ScalarDomain::Raw, AbiType::U64) => PrimitiveType::U64,
        (ScalarDomain::Raw, AbiType::F32) => PrimitiveType::F32,
        (ScalarDomain::Raw, AbiType::F64) => PrimitiveType::F64,
        (ScalarDomain::Bool, AbiType::U8) => PrimitiveType::Bool,
        (ScalarDomain::UnicodeScalar, AbiType::U32) => PrimitiveType::Char,
        (ScalarDomain::NativeWord, AbiType::USize) => PrimitiveType::Word,
        _ => return None,
    })
}
/// Lower one admitted Glue logical type to its Interop.Contracts shape. This is the only
/// lowering: record layout comes from the C ABI profile here and nowhere else.
pub fn glue_type_shape(ty: &GlueLogicalType) -> Result<TypeShape, SemanticError> {
    use beskid_abi::{abi_v5::AbiType, interop::*};
    let profile = c_profile::CAbiProfile;
    let shape = match ty {
        GlueLogicalType::Primitive(PrimitiveType::String) => TypeShape::StringLike,
        GlueLogicalType::Primitive(PrimitiveType::Unit) => TypeShape::Unit,
        GlueLogicalType::Primitive(primitive) => TypeShape::Scalar(
            glue_scalar_shape(*primitive).ok_or_else(|| invalid("raw pointer and never are unsupported Glue signatures"))?,
        ),
        GlueLogicalType::Array(element) if **element == GlueLogicalType::Primitive(PrimitiveType::U8) => {
            TypeShape::Buffer(BufferShape { element_type: AbiType::U8, is_utf8: false })
        }
        GlueLogicalType::Array(_) | GlueLogicalType::Generic { .. } => {
            GlueLogicalForm::of(ty)?;
            return Err(invalid("unadmitted Glue form"));
        }
        GlueLogicalType::Handle(brand) => TypeShape::BrandedHandle(BrandedHandleShape {
            identity: brand.qualified_identity.to_string(),
            nullable: brand.nullable,
            arguments: brand.arguments.iter().map(glue_type_shape).collect::<Result<_, _>>()?,
        }),
        GlueLogicalType::Record(record) => {
            let fields = record
                .fields
                .iter()
                .map(|field| {
                    glue_scalar_shape(field.ty)
                        .map(|scalar| (field.name.clone(), scalar))
                        .ok_or_else(|| invalid("GlueRecord field is not an admitted scalar"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            TypeShape::Record(
                profile
                    .record_shape(&record.qualified_identity, &fields)
                    .map_err(|error| invalid(&format!("GlueRecord layout: {error}")))?,
            )
        }
        GlueLogicalType::Enum(enumeration) => TypeShape::Enumeration(EnumerationShape {
            identity: enumeration.qualified_identity.to_string(),
            variants: enumeration.variants.to_vec(),
        }),
        GlueLogicalType::Callback(callback) => TypeShape::Callback(CallbackShape {
            parameters: callback.parameters.iter().map(glue_type_shape).collect::<Result<_, _>>()?,
            result: match callback.result.as_ref() {
                GlueLogicalType::Primitive(PrimitiveType::Unit) => None,
                result => Some(Box::new(glue_type_shape(result)?)),
            },
        }),
    };
    if !profile.bind(&shape).permitted {
        return Err(invalid("the C ABI profile rejects the lowered Glue shape"));
    }
    Ok(shape)
}
/// Lift a boundary shape back to the Glue logical form it was lowered from. `None` for a shape
/// that no admitted Glue form lowers to.
pub fn glue_logical_form(shape: &TypeShape) -> Option<GlueLogicalForm> {
    use beskid_abi::abi_v5::AbiType;
    Some(match shape {
        TypeShape::Scalar(scalar) => GlueLogicalForm::Primitive(glue_scalar_primitive(scalar)?),
        TypeShape::StringLike => GlueLogicalForm::Primitive(PrimitiveType::String),
        TypeShape::Unit => GlueLogicalForm::Primitive(PrimitiveType::Unit),
        TypeShape::Buffer(buffer) if buffer.element_type == AbiType::U8 && !buffer.is_utf8 => GlueLogicalForm::Bytes,
        TypeShape::BrandedHandle(handle) => GlueLogicalForm::Handle {
            identity: handle.identity.clone(),
            nullable: handle.nullable,
            arguments: handle.arguments.iter().map(glue_logical_form).collect::<Option<_>>()?,
        },
        TypeShape::Record(record) => GlueLogicalForm::Record {
            identity: record.identity.clone(),
            fields: record
                .fields
                .iter()
                .map(|field| glue_scalar_primitive(&field.scalar).map(|primitive| (field.name.clone(), primitive)))
                .collect::<Option<_>>()?,
        },
        TypeShape::Enumeration(enumeration) => GlueLogicalForm::Enum {
            identity: enumeration.identity.clone(),
            variants: enumeration.variants.clone(),
        },
        TypeShape::Callback(callback) => GlueLogicalForm::Callback {
            parameters: callback.parameters.iter().map(glue_logical_form).collect::<Option<_>>()?,
            result: Box::new(match &callback.result {
                Some(result) => glue_logical_form(result)?,
                None => GlueLogicalForm::Primitive(PrimitiveType::Unit),
            }),
        },
        TypeShape::Buffer(_) | TypeShape::OpaqueHandle | TypeShape::Never => return None,
    })
}
/// Lower a Glue binding to its Interop.Contracts signature: one parameter per source parameter
/// and the result, with the call shape and ownership of the Glue C ABI transport.
pub fn glue_interop_signature(fact: &GlueBindingFact) -> Result<InteropSignature, SemanticError> {
    use beskid_abi::interop::*;
    let parameters = fact
        .parameters
        .iter()
        .map(|parameter| {
            let ty = glue_type_shape(&parameter.ty)?;
            let (call, ownership) = match &ty {
                TypeShape::StringLike | TypeShape::Buffer(_) => (CallShapeClass::View, OwnershipClass::Borrow),
                TypeShape::BrandedHandle(_) => (CallShapeClass::Direct, OwnershipClass::OpaqueBorrow),
                _ => (CallShapeClass::Direct, OwnershipClass::Borrow),
            };
            Ok(InteropParameter { name: parameter.name.clone(), ty, call, ownership })
        })
        .collect::<Result<Vec<_>, SemanticError>>()?;
    let returns = glue_type_shape(&fact.result)?;
    let ownership = match &returns {
        TypeShape::StringLike | TypeShape::Buffer(_) | TypeShape::BrandedHandle(_) => OwnershipClass::Transfer,
        _ => OwnershipClass::Borrow,
    };
    let signature = InteropSignature {
        symbol: fact.symbol.clone(),
        parameters,
        returns: InteropReturn { name: None, ty: returns, ownership },
        no_return: false,
    };
    signature.validate().map_err(|error| invalid(&format!("Glue interop signature: {error}")))?;
    c_profile::CAbiProfile
        .validate_signature(&signature)
        .map_err(|error| invalid(&format!("Glue interop signature: {error}")))?;
    Ok(signature)
}
