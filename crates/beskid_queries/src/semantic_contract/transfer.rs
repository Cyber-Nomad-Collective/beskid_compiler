//! Transfer classes (`language-meta--contracts-and-effects--contracts`, "Transfer class query").
//!
//! Every type identity has exactly one transfer class: Shared, Moved, Copied or Never(reason).
//! A type parameter inside its generic definition takes the class of its marker bound, where
//! `Transferable` gives "some Transferable". The marker contracts `Transferable`, `Shared`,
//! `Moved` and `Copied` are proven through this query, never by declaration.
use super::contracts::{concrete_type_or_enum_declaration, declared_where_bounds};
use super::*;
use std::fmt;

/// The transfer class of one type identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TransferClass {
    Shared,
    Moved,
    Copied,
    /// A type parameter bounded by `Transferable` inside its generic definition: one of Shared,
    /// Moved or Copied, decided per instantiation. It satisfies only the `Transferable` bound.
    SomeTransferable,
    Never(TransferNeverReason),
}

/// Why a type cannot cross a heap boundary.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TransferNeverReason {
    /// `pointer`.
    NativePointer,
    /// `never`.
    NeverType,
    /// A function type.
    FunctionType,
    /// `T[]` whose element type is Never.
    ArrayElement(Box<TransferNeverReason>),
    /// A nominal type carrying `[NeverTransferable]`.
    NeverTransferable { type_name: String },
    /// A nominal type with a field whose class the type's marker (or Never) rejects.
    Field { type_name: String, field: String, class: Box<TransferClass> },
    /// A nominal type without `[Immutable]`, `[Isolated]` or `[Copyable]`.
    NoMarker { type_name: String },
    /// An enum whose variant fields mix Moved and Copied classes.
    MixedEnum { type_name: String },
    /// A type parameter without a marker bound.
    UnboundedTypeParameter { name: String },
}

/// The four compiler-derived marker contracts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransferMarker {
    Transferable,
    Shared,
    Moved,
    Copied,
}

impl TransferMarker {
    pub const ALL: [TransferMarker; 4] = [Self::Transferable, Self::Shared, Self::Moved, Self::Copied];

    pub fn name(self) -> &'static str {
        match self {
            Self::Transferable => "Transferable",
            Self::Shared => "Shared",
            Self::Moved => "Moved",
            Self::Copied => "Copied",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|marker| marker.name() == name)
    }
}

impl TransferClass {
    /// Shared, Moved, Copied or some Transferable.
    pub fn is_transferable(&self) -> bool {
        !matches!(self, Self::Never(_))
    }

    /// Whether a value of this class satisfies the marker bound `marker`.
    pub fn satisfies(&self, marker: TransferMarker) -> bool {
        match (self, marker) {
            (Self::Never(_), _) => false,
            (_, TransferMarker::Transferable) => true,
            (Self::Shared, TransferMarker::Shared)
            | (Self::Moved, TransferMarker::Moved)
            | (Self::Copied, TransferMarker::Copied) => true,
            _ => false,
        }
    }

    /// The class name used in diagnostics and hover.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Shared => "Shared",
            Self::Moved => "Moved",
            Self::Copied => "Copied",
            Self::SomeTransferable => "some Transferable",
            Self::Never(_) => "Never",
        }
    }
}

impl fmt::Display for TransferNeverReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NativePointer => f.write_str("`pointer` is a native address"),
            Self::NeverType => f.write_str("`never` has no values"),
            Self::FunctionType => f.write_str("function values are heap-local"),
            Self::ArrayElement(reason) => write!(f, "its element type is not transferable: {reason}"),
            Self::NeverTransferable { .. } => f.write_str("`[NeverTransferable]`"),
            Self::Field { field, class, .. } => match class.as_ref() {
                TransferClass::Never(reason) => write!(f, "field `{field}` is Never ({reason})"),
                other => write!(f, "field `{field}` has class {}", other.name()),
            },
            Self::NoMarker { .. } => f.write_str("no marker attribute"),
            Self::MixedEnum { type_name } => write!(f, "enum `{type_name}` mixes Moved and Copied fields"),
            Self::UnboundedTypeParameter { name } => write!(f, "type parameter `{name}` has no marker bound"),
        }
    }
}

impl fmt::Display for TransferClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Never(reason) => write!(f, "Never ({reason})"),
            other => f.write_str(other.name()),
        }
    }
}

/// The marker attributes the transfer class reads from a nominal declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransferMarkerAttribute {
    Immutable,
    Isolated,
    Copyable,
}

impl TransferMarkerAttribute {
    pub const ALL: [TransferMarkerAttribute; 3] = [Self::Immutable, Self::Isolated, Self::Copyable];

    pub fn name(self) -> &'static str {
        match self {
            Self::Immutable => "Immutable",
            Self::Isolated => "Isolated",
            Self::Copyable => "Copyable",
        }
    }

    /// The class the marker gives a type whose fields it admits.
    pub fn class(self) -> TransferClass {
        match self {
            Self::Immutable => TransferClass::Shared,
            Self::Isolated => TransferClass::Moved,
            Self::Copyable => TransferClass::Copied,
        }
    }

    /// Whether the marker admits a field of class `class` (`[ImmutableStorage]` is judged apart).
    pub fn admits(self, class: &TransferClass) -> bool {
        match self {
            Self::Immutable => matches!(class, TransferClass::Shared),
            Self::Isolated => matches!(class, TransferClass::Shared | TransferClass::Moved),
            Self::Copyable => matches!(class, TransferClass::Shared | TransferClass::Copied),
        }
    }

    /// The classes the marker admits, for diagnostics.
    pub fn admitted(self) -> &'static str {
        match self {
            Self::Immutable => "Shared",
            Self::Isolated => "Shared and Moved",
            Self::Copyable => "Shared and Copied",
        }
    }
}

fn unit_root_of(key: AstNodeKey) -> AstNodeKey {
    AstNodeKey { node: beskid_analysis::syntax::AstNodeId(0), ..key }
}

/// Whether `attributes`, written in the unit of `key`, contain a use of the compiler-recognized
/// attribute `name` (a user declaration that shadows the name is not the recognized attribute).
pub(in crate::semantic_contract) fn has_recognized_attribute(
    db: &dyn Db,
    key: AstNodeKey,
    attributes: &[beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Attribute>],
    name: &str,
) -> bool {
    recognized_attribute_use(db, key, attributes, name).is_some()
}

/// Why a type declaration is never transferable by declaration: `[NeverTransferable]`, or
/// `[GlueHandle]`, which brands a Glue owner handle and implies `[NeverTransferable]` (v0.6.1).
pub(in crate::semantic_contract) fn never_transferable_cause(
    db: &dyn Db,
    key: AstNodeKey,
    attributes: &[beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Attribute>],
) -> Option<&'static str> {
    if has_recognized_attribute(db, key, attributes, "NeverTransferable") {
        Some("`[NeverTransferable]`")
    } else if has_recognized_attribute(db, key, attributes, "GlueHandle") {
        Some("`[GlueHandle]` (a Glue owner)")
    } else {
        None
    }
}

/// The first use of the compiler-recognized attribute `name` in `attributes`.
pub(in crate::semantic_contract) fn recognized_attribute_use<'a>(
    db: &dyn Db,
    key: AstNodeKey,
    attributes: &'a [beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Attribute>],
    name: &str,
) -> Option<&'a beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Attribute>> {
    attributes.iter().find(|attribute| {
        attribute.node.name.node.name == name && resolve_attribute_use(db, unit_root_of(key), name).is_recognized()
    })
}

/// The indexed node of `target`, a syntax value borrowed from the expanded program of `owner`'s
/// unit at or below `owner` (an attribute, field, conformance or contract member). Expanded
/// syntax values carry no identity of their own (`Spanned::id` stays unset), so a finding site is
/// found through the syntax index; `owner` itself when `target` is not below it.
pub(in crate::semantic_contract) fn indexed_site(
    db: &dyn Db,
    owner: AstNodeKey,
    target: beskid_analysis::syntax_query::DynNodeRef<'_>,
) -> AstNodeKey {
    let Some(syntax) = db.syntax_unit(owner.unit).filter(|syntax| syntax.accepts_key(db, owner)) else { return owner };
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    let wanted = std::ptr::from_ref(target.0).cast::<()>();
    let mut pending = std::collections::VecDeque::from([owner.node]);
    while let Some(node) = pending.pop_front() {
        if index.node_at(program, node).is_some_and(|candidate| std::ptr::eq(std::ptr::from_ref(candidate.0).cast::<()>(), wanted)) {
            return AstNodeKey { node, ..owner };
        }
        pending.extend(index.children(node).into_iter().flatten().copied());
    }
    owner
}

/// The marker attributes on a declaration, in source order.
pub(in crate::semantic_contract) fn marker_attributes(
    db: &dyn Db,
    key: AstNodeKey,
    attributes: &[beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Attribute>],
) -> Vec<(TransferMarkerAttribute, AstNodeKey)> {
    attributes
        .iter()
        .filter_map(|attribute| {
            let marker = TransferMarkerAttribute::ALL
                .into_iter()
                .find(|marker| marker.name() == attribute.node.name.node.name)?;
            resolve_attribute_use(db, unit_root_of(key), marker.name())
                .is_recognized()
                .then(|| (marker, indexed_site(db, key, beskid_analysis::syntax_query::DynNodeRef::from(attribute))))
        })
        .collect()
}

/// The marker contract a contract declaration is: one of the four names, carrying the
/// recognized `[CompilerDerived]`.
pub fn transfer_marker_contract(db: &dyn Db, contract: AstNodeKey) -> Option<TransferMarker> {
    let syntax = db.syntax_unit(contract.unit).filter(|syntax| syntax.accepts_key(db, contract))?;
    let definition = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), contract.node)?
        .of::<beskid_analysis::syntax::ContractDefinition>()?;
    let marker = TransferMarker::from_name(&definition.name.node.name)?;
    has_recognized_attribute(db, contract, &definition.attributes, "CompilerDerived").then_some(marker)
}

/// One generic environment of the class computation: type parameter name to class.
type ClassEnvironment = HashMap<String, TransferClass>;

/// The class computation with the instantiations in progress, so a recursive type
/// (`[Isolated] type Node { Option<Node> next }`) assumes its marker's class coinductively.
struct ClassWalker<'db> {
    db: &'db dyn Db,
    in_progress: Vec<(AstNodeKey, Vec<TransferClass>)>,
}

fn abi_class(ty: SemanticTypeId) -> TransferClass {
    match ty {
        SemanticTypeId::POINTER => TransferClass::Never(TransferNeverReason::NativePointer),
        SemanticTypeId::NEVER => TransferClass::Never(TransferNeverReason::NeverType),
        _ => TransferClass::Shared,
    }
}

/// Rule 3: `T[]` is Moved when `T` is transferable.
fn array_class(element: TransferClass) -> TransferClass {
    match element {
        TransferClass::Never(reason) => TransferClass::Never(TransferNeverReason::ArrayElement(Box::new(reason))),
        _ => TransferClass::Moved,
    }
}

/// Rule 9 over the classes of every variant field.
fn enum_class(type_name: &str, fields: &[(String, TransferClass)]) -> TransferClass {
    if let Some((field, class)) = fields.iter().find(|(_, class)| !class.is_transferable()) {
        return TransferClass::Never(TransferNeverReason::Field {
            type_name: type_name.to_owned(),
            field: field.clone(),
            class: Box::new(class.clone()),
        });
    }
    let moved = fields.iter().any(|(_, class)| *class == TransferClass::Moved);
    let copied = fields.iter().any(|(_, class)| *class == TransferClass::Copied);
    let some = fields.iter().any(|(_, class)| *class == TransferClass::SomeTransferable);
    match (moved, copied, some) {
        (true, true, _) => TransferClass::Never(TransferNeverReason::MixedEnum { type_name: type_name.to_owned() }),
        // A variant field of a `Transferable`-bounded parameter is decided per instantiation.
        (false, false, true) => TransferClass::SomeTransferable,
        (true, false, true) | (false, true, true) => {
            TransferClass::Never(TransferNeverReason::MixedEnum { type_name: type_name.to_owned() })
        }
        (true, false, false) => TransferClass::Moved,
        (false, true, false) => TransferClass::Copied,
        (false, false, false) => TransferClass::Shared,
    }
}

impl<'db> ClassWalker<'db> {
    fn new(db: &'db dyn Db) -> Self {
        Self { db, in_progress: Vec::new() }
    }

    /// The class of a source type written at `context`, with the generic environment `environment`.
    fn syntax_class(
        &mut self,
        context: AstNodeKey,
        ty: &beskid_analysis::syntax::Type,
        environment: &ClassEnvironment,
    ) -> Result<TransferClass, SemanticError> {
        use beskid_analysis::syntax::Type;
        if let Some(parameter) = generic_parameter_reference_name(ty)
            && let Some(class) = environment.get(parameter)
        {
            return Ok(class.clone());
        }
        match ty {
            Type::Primitive(_) => Ok(abi_class(abi_type_from_syntax(self.db, context, ty)?)),
            Type::Array(element) => Ok(array_class(self.syntax_class(context, &element.node, environment)?)),
            Type::Function { .. } => Ok(TransferClass::Never(TransferNeverReason::FunctionType)),
            Type::This => {
                let receiver =
                    method_this_type(self.db, context).ok_or_else(|| SemanticError::unavailable_at("transfer_class", context))?;
                self.syntax_class(context, &receiver, environment)
            }
            Type::Associated { .. } => Err(SemanticError::unavailable_at("transfer_class", context)),
            Type::Complex(path) => {
                let declaration = resolve_type_declaration(self.db, context, &path.node)
                    .ok_or_else(|| SemanticError::unavailable_at("transfer_class", context))?;
                let arguments = path
                    .node
                    .segments
                    .iter()
                    .flat_map(|segment| segment.node.type_args.iter())
                    .map(|argument| self.syntax_class(context, &argument.node, environment))
                    .collect::<Result<Vec<_>, _>>()?;
                self.nominal_class(declaration, arguments)
            }
        }
    }

    /// The class of a fully substituted source identity, read in the assembly of `context`.
    fn identity_class(
        &mut self,
        context: AstNodeKey,
        identity: &GenericSourceTypeIdentity,
    ) -> Result<TransferClass, SemanticError> {
        match identity {
            GenericSourceTypeIdentity::Abi(ty) => Ok(abi_class(*ty)),
            GenericSourceTypeIdentity::Array(element) => Ok(array_class(self.identity_class(context, element)?)),
            GenericSourceTypeIdentity::Function { .. } => Ok(TransferClass::Never(TransferNeverReason::FunctionType)),
            GenericSourceTypeIdentity::Nominal { arguments, .. } => {
                let declaration = concrete_type_or_enum_declaration(self.db, context, identity)
                    .ok_or_else(|| SemanticError::unavailable_at("transfer_class", context))?;
                let arguments = arguments
                    .iter()
                    .map(|argument| self.identity_class(context, argument))
                    .collect::<Result<Vec<_>, _>>()?;
                self.nominal_class(declaration, arguments)
            }
        }
    }

    /// Rules 4-10 for one nominal instantiation whose arguments have the classes `arguments`.
    fn nominal_class(
        &mut self,
        declaration: AstNodeKey,
        arguments: Vec<TransferClass>,
    ) -> Result<TransferClass, SemanticError> {
        let db = self.db;
        let unavailable = || SemanticError::unavailable_at("transfer_class", declaration);
        let syntax = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration)).ok_or_else(unavailable)?;
        let node = syntax.syntax_index(db).node_at(syntax.expanded_program(db), declaration.node).ok_or_else(unavailable)?;
        if let Some(definition) = node.of::<beskid_analysis::syntax::TypeDefinition>() {
            let type_name = definition.name.node.name.clone();
            if never_transferable_cause(db, declaration, &definition.attributes).is_some() {
                return Ok(TransferClass::Never(TransferNeverReason::NeverTransferable { type_name }));
            }
            let marker = marker_attributes(db, declaration, &definition.attributes).first().map(|(marker, _)| *marker);
            if self.in_progress.iter().any(|(key, classes)| *key == declaration && *classes == arguments) {
                return Ok(marker
                    .map(TransferMarkerAttribute::class)
                    .unwrap_or(TransferClass::Never(TransferNeverReason::NoMarker { type_name })));
            }
            let environment = generic_environment(&definition.generics, &arguments).ok_or_else(unavailable)?;
            self.in_progress.push((declaration, arguments));
            let fields = self.field_classes(declaration, &definition.fields, &environment);
            self.in_progress.pop();
            let fields = fields?;
            // Rule 5, then the marker rules 6-8, then rule 10.
            if let Some((field, class, _)) = fields.iter().find(|(_, class, _)| !class.is_transferable()) {
                return Ok(field_never(&type_name, field, class));
            }
            let Some(marker) = marker else {
                return Ok(TransferClass::Never(TransferNeverReason::NoMarker { type_name }));
            };
            for (field, class, immutable_storage) in &fields {
                let admitted = marker.admits(class)
                    || (marker == TransferMarkerAttribute::Immutable && *immutable_storage);
                if !admitted {
                    return Ok(field_never(&type_name, field, class));
                }
            }
            return Ok(marker.class());
        }
        let definition = node.of::<beskid_analysis::syntax::EnumDefinition>().ok_or_else(unavailable)?;
        let type_name = definition.name.node.name.clone();
        if self.in_progress.iter().any(|(key, classes)| *key == declaration && *classes == arguments) {
            // A recursive enum (`List<T> = Cons(T, List<T>) | Nil`) adds nothing to its own class.
            return Ok(TransferClass::Shared);
        }
        let environment = generic_environment(&definition.generics, &arguments).ok_or_else(unavailable)?;
        self.in_progress.push((declaration, arguments));
        let mut fields = Vec::new();
        let mut result = Ok(());
        for variant in &definition.variants {
            match self.field_classes(declaration, &variant.node.fields, &environment) {
                Ok(classes) => fields.extend(classes.into_iter().map(|(field, class, _)| (field, class))),
                Err(error) => {
                    result = Err(error);
                    break;
                }
            }
        }
        self.in_progress.pop();
        result?;
        Ok(enum_class(&type_name, &fields))
    }

    /// Every field's name, class and whether it is an `[ImmutableStorage]` `T[]` with `T` Shared.
    fn field_classes(
        &mut self,
        declaration: AstNodeKey,
        fields: &[beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Field>],
        environment: &ClassEnvironment,
    ) -> Result<Vec<(String, TransferClass, bool)>, SemanticError> {
        let scope_borrowed = scope_borrowed_declaration(self.db, declaration);
        fields
            .iter()
            .map(|field| {
                let name = field.node.name.node.name.clone();
                if field.node.kind == beskid_analysis::syntax::FieldKind::Event {
                    return Ok((name, TransferClass::Never(TransferNeverReason::FunctionType), false));
                }
                // A function-typed field with `[Captures(Bound: C)]` holds only closures whose
                // captures satisfy C, so it has the bound's class (`par` R-lang-3).
                let bounded = match &field.node.ty.node {
                    beskid_analysis::syntax::Type::Function { .. } => {
                        match captures_bound(self.db, declaration, &field.node.attributes) {
                            Some((CapturesBound::Marker(marker), _)) => Some(bound_class(&name, &[marker])),
                            _ => None,
                        }
                    }
                    _ => None,
                };
                let class = match bounded {
                    Some(class) => class,
                    None => self.syntax_class(declaration, &field.node.ty.node, environment)?,
                };
                let immutable_storage = match &field.node.ty.node {
                    beskid_analysis::syntax::Type::Array(element)
                        if has_recognized_attribute(self.db, declaration, &field.node.attributes, "ImmutableStorage") =>
                    {
                        let element = self.syntax_class(declaration, &element.node, environment)?;
                        // A corelib `[ScopeBorrowed]` view borrows storage of any transferable
                        // element for the lifetime of a scope (`par` R-lang-4, v065 sequences).
                        element == TransferClass::Shared || (scope_borrowed && element.is_transferable())
                    }
                    _ => false,
                };
                Ok((name, class, immutable_storage))
            })
            .collect()
    }
}

/// Whether the type declaration carries the corelib-only `[ScopeBorrowed]`.
fn scope_borrowed_declaration(db: &dyn Db, declaration: AstNodeKey) -> bool {
    let Some(syntax) = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration)) else {
        return false;
    };
    syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
        .is_some_and(|definition| has_recognized_attribute(db, declaration, &definition.attributes, "ScopeBorrowed"))
}

fn field_never(type_name: &str, field: &str, class: &TransferClass) -> TransferClass {
    TransferClass::Never(TransferNeverReason::Field {
        type_name: type_name.to_owned(),
        field: field.to_owned(),
        class: Box::new(class.clone()),
    })
}

fn generic_environment(
    generics: &[beskid_analysis::syntax::Spanned<beskid_analysis::syntax::TypeParameter>],
    arguments: &[TransferClass],
) -> Option<ClassEnvironment> {
    (generics.len() == arguments.len()).then(|| {
        generics.iter().zip(arguments).map(|(generic, class)| (generic.node.name.clone(), class.clone())).collect()
    })
}

/// The class a type parameter takes inside its generic definition from its marker bounds.
pub(in crate::semantic_contract) fn bound_class(name: &str, markers: &[TransferMarker]) -> TransferClass {
    let class = markers.iter().find_map(|marker| match marker {
        TransferMarker::Shared => Some(TransferClass::Shared),
        TransferMarker::Moved => Some(TransferClass::Moved),
        TransferMarker::Copied => Some(TransferClass::Copied),
        TransferMarker::Transferable => None,
    });
    match class {
        Some(class) => class,
        None if markers.contains(&TransferMarker::Transferable) => TransferClass::SomeTransferable,
        None => TransferClass::Never(TransferNeverReason::UnboundedTypeParameter { name: name.to_owned() }),
    }
}

/// The class of every type parameter in scope at `site`: the enclosing callable's own and owner
/// parameters with their `where` marker bounds, and the enclosing type's parameters (unbounded).
fn scope_environment(db: &dyn Db, site: AstNodeKey) -> ClassEnvironment {
    let mut environment = ClassEnvironment::new();
    let Some(syntax) = db.syntax_unit(site.unit).filter(|syntax| syntax.accepts_key(db, site)) else {
        return environment;
    };
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    use beskid_analysis::syntax_query::NodeKind;
    let owner = nearest_ancestor(index, site.node, |kind| {
        matches!(kind, NodeKind::TypeDefinition | NodeKind::EnumDefinition | NodeKind::ImplBlock)
    })
    .or_else(|| matches!(index.kind(site.node), Some(NodeKind::TypeDefinition | NodeKind::EnumDefinition)).then_some(site.node));
    if let Some(node) = owner.and_then(|owner| index.node_at(program, owner)) {
        let generics = node
            .of::<beskid_analysis::syntax::TypeDefinition>()
            .map(|definition| &definition.generics)
            .or_else(|| node.of::<beskid_analysis::syntax::EnumDefinition>().map(|definition| &definition.generics))
            .or_else(|| node.of::<beskid_analysis::syntax::ImplBlock>().map(|block| &block.generics));
        for generic in generics.into_iter().flatten() {
            environment.insert(generic.node.name.clone(), bound_class(&generic.node.name, &[]));
        }
    }
    let callable = nearest_ancestor(index, site.node, |kind| {
        matches!(kind, NodeKind::FunctionDefinition | NodeKind::MethodDefinition)
    })
    .or_else(|| {
        matches!(index.kind(site.node), Some(NodeKind::FunctionDefinition | NodeKind::MethodDefinition)).then_some(site.node)
    });
    if let Some(callable) = callable {
        let callable = AstNodeKey { node: callable, ..site };
        if let Some((names, _)) = generic_callable_parameters(db, callable) {
            for name in names {
                environment.insert(name.to_owned(), bound_class(name, &[]));
            }
        }
        let bounds = declared_where_bounds(db, callable).unwrap_or_default();
        let names = environment.keys().cloned().collect::<Vec<_>>();
        for name in names {
            let markers = bounds
                .iter()
                .filter(|bound| bound.generic == name)
                .filter_map(|bound| transfer_marker_contract(db, bound.contract))
                .collect::<Vec<_>>();
            environment.insert(name.clone(), bound_class(&name, &markers));
        }
    }
    environment
}

/// The transfer class of a nominal declaration instantiated with `arguments` (in declaration
/// order; empty for a non-generic type).
pub fn nominal_transfer_class(
    db: &dyn Db,
    declaration: AstNodeKey,
    arguments: &[GenericSubstitution],
) -> Result<TransferClass, SemanticError> {
    let mut walker = ClassWalker::new(db);
    let arguments = arguments
        .iter()
        .map(|argument| walker.identity_class(declaration, argument.source_identity()))
        .collect::<Result<Vec<_>, _>>()?;
    walker.nominal_class(declaration, arguments)
}

/// The transfer class of the record built by the struct literal `literal`, instantiated in the
/// generic specialization `enclosing` (type arguments written on the literal path may name the
/// enclosing parameters). `None` when the application is not exactly provable; callers then
/// attach no property bit.
pub fn aggregate_literal_transfer_class(
    db: &dyn Db,
    literal: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Option<TransferClass> {
    let syntax = db.syntax_unit(literal.unit).filter(|syntax| syntax.accepts_key(db, literal))?;
    let node = syntax.syntax_index(db).node_at(syntax.expanded_program(db), literal.node)?;
    let expression = node.of::<beskid_analysis::syntax::StructLiteralExpression>()?;
    let environment = enclosing
        .map(|enclosing| {
            enclosing
                .substitutions
                .iter()
                .map(|binding| (binding.parameter.as_ref(), binding.source_identity()))
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();
    let identity = generic_source_type_identity_with_substitutions(
        db,
        literal,
        &beskid_analysis::syntax::Type::Complex(expression.path.clone()),
        &environment,
    )
    .ok()?;
    transfer_class(db, literal, &identity).ok()
}

/// The transfer class of the enum value built by the constructor `constructor`, instantiated in
/// the generic specialization `enclosing`. `None` when the application is not exactly provable.
pub fn enum_constructor_transfer_class(
    db: &dyn Db,
    constructor: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Option<TransferClass> {
    let syntax = db.syntax_unit(constructor.unit).filter(|syntax| syntax.accepts_key(db, constructor))?;
    let node = syntax.syntax_index(db).node_at(syntax.expanded_program(db), constructor.node)?;
    let expression = node.of::<beskid_analysis::syntax::EnumConstructorExpression>()?;
    let identity =
        applied_enum_constructor_identity(db, constructor, expression, enclosing).ok()?;
    transfer_class(db, constructor, &identity).ok()
}

/// The transfer class of a source type written at `site`. Type parameters in scope at `site`
/// take the class of their marker bound.
pub fn transfer_class_of_type(
    db: &dyn Db,
    site: AstNodeKey,
    ty: &beskid_analysis::syntax::Type,
) -> Result<TransferClass, SemanticError> {
    let environment = scope_environment(db, site);
    ClassWalker::new(db).syntax_class(site, ty, &environment)
}

/// The transfer class of a fully substituted source identity, read in the assembly of `context`.
pub(in crate::semantic_contract) fn transfer_class(
    db: &dyn Db,
    context: AstNodeKey,
    identity: &GenericSourceTypeIdentity,
) -> Result<TransferClass, SemanticError> {
    ClassWalker::new(db).identity_class(context, identity)
}

/// The reason text of an E1610 for a marker bound `marker` that `class` does not satisfy.
pub(in crate::semantic_contract) fn marker_bound_reason(class: &TransferClass, marker: TransferMarker) -> String {
    match class {
        TransferClass::Never(reason) => reason.to_string(),
        other => format!("class {}, bound requires {}", other.name(), marker.name()),
    }
}

/// The class of one declared field of the nominal at `declaration` whose type parameters are
/// `generics`, and whether it is an `[ImmutableStorage]` `T[]` with `T` Shared. `None` when the
/// field's type mentions a type parameter of the declaration: such a field is judged per
/// instantiation, never at the declaration.
pub(in crate::semantic_contract) fn declared_field_class(
    db: &dyn Db,
    declaration: AstNodeKey,
    generics: &[beskid_analysis::syntax::Spanned<beskid_analysis::syntax::TypeParameter>],
    field: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Field>,
) -> Result<Option<(TransferClass, bool)>, SemanticError> {
    if field.node.kind == beskid_analysis::syntax::FieldKind::Event {
        return Ok(Some((TransferClass::Never(TransferNeverReason::FunctionType), false)));
    }
    if generics.iter().any(|generic| type_syntax_mentions_generic_parameter(&field.node.ty.node, &generic.node.name)) {
        return Ok(None);
    }
    let mut walker = ClassWalker::new(db);
    let environment = ClassEnvironment::new();
    let mut classes = walker.field_classes(declaration, std::slice::from_ref(field), &environment)?;
    Ok(classes.pop().map(|(_, class, immutable_storage)| (class, immutable_storage)))
}
