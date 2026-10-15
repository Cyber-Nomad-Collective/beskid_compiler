//! Transfer classes (`language-meta--contracts-and-effects--contracts`, "Transfer class query").
//!
//! Every type identity has exactly one transfer class: Shared, Moved, Copied or Never(reason).
//! A type parameter inside its generic definition takes the class of its marker bound, where
//! `Transferable` gives "some Transferable". The marker contracts `Transferable`, `Shared`,
//! `Moved` and `Copied` are proven through this query, never by declaration.
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
            Self::NeverTransferable { type_name } => write!(f, "`{type_name}` is `[NeverTransferable]`"),
            Self::Field { type_name, field, class } => {
                write!(f, "field `{field}` of `{type_name}` has class {}", class.name())
            }
            Self::NoMarker { type_name } => write!(f, "`{type_name}` has no marker attribute"),
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

/// The transfer class of a nominal declaration instantiated with `arguments` (in declaration
/// order; empty for a non-generic type).
pub fn nominal_transfer_class(
    db: &dyn Db,
    declaration: AstNodeKey,
    arguments: &[GenericSubstitution],
) -> Result<TransferClass, SemanticError> {
    let _ = (db, arguments);
    Err(SemanticError::unavailable_at("transfer_class", declaration))
}

/// The transfer class of a source type written at `site`. Type parameters in scope at `site`
/// take the class of their marker bound.
pub fn transfer_class_of_type(
    db: &dyn Db,
    site: AstNodeKey,
    ty: &beskid_analysis::syntax::Type,
) -> Result<TransferClass, SemanticError> {
    let _ = (db, ty);
    Err(SemanticError::unavailable_at("transfer_class", site))
}
