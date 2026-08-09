use crate::facts::{AstNodeKey, DirectCallee};

/// Artifact-owned string materialization invoked only after generated ISLE selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StringMaterializationError {
    MissingDispatchRoute(&'static str),
    DispatchEmission(&'static str),
    Artifact(&'static str),
}

impl std::fmt::Display for StringMaterializationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingDispatchRoute(symbol) => {
                write!(f, "MissingDispatchRoute({symbol})")
            }
            Self::DispatchEmission(detail) => write!(f, "DispatchEmission({detail})"),
            Self::Artifact(detail) => write!(f, "Artifact({detail})"),
        }
    }
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoweringErrorKind {
    MissingRuleOrFact,
    StringMaterialization(StringMaterializationError),
    UnknownCallee(DirectCallee),
    InvalidPrimitiveNumericConversion(&'static str),
    InvalidArrayLayout,
    InvalidStructLayout,
    InvalidStructField(u32),
    InvalidEnumLayout,
    InvalidEnumVariant(u32),
    InvalidMatchArms,
    NonExhaustiveMatch,
    InvalidBlockExpression,
    InvalidRangeFor,
}


#[derive(Clone, PartialEq, Eq)]
pub struct LoweringError {
    pub(crate) key: AstNodeKey,
    pub(crate) kind: LoweringErrorKind,
}

impl LoweringError {
    pub fn key(&self) -> AstNodeKey {
        self.key
    }

    pub fn kind(&self) -> LoweringErrorKind {
        self.kind.clone()
    }

    /// Prefer this when a Salsa db is available so the unit path is expanded.
    pub fn display_with_key_label(&self, key_label: impl std::fmt::Display) -> String {
        format!("{} at {key_label}", self.kind_label())
    }

    /// Expand the failing site with path, AST construct, and source range when a db is available.
    ///
    /// Example: `MissingRuleOrFact at Assert.bd#g1:n19 IfStatement@52:5-55:6`.
    pub fn display_with_db(&self, db: &dyn beskid_queries::Db) -> String {
        self.display_with_key_label(beskid_queries::format_ast_node_site(db, self.key))
    }

    fn kind_label(&self) -> String {
        match &self.kind {
            LoweringErrorKind::MissingRuleOrFact => "MissingRuleOrFact".to_owned(),
            LoweringErrorKind::StringMaterialization(error) => {
                format!("StringMaterialization({error})")
            }
            LoweringErrorKind::UnknownCallee(callee) => format!("UnknownCallee({callee:?})"),
            LoweringErrorKind::InvalidPrimitiveNumericConversion(reason) => {
                format!("InvalidPrimitiveNumericConversion({reason})")
            }
            LoweringErrorKind::InvalidArrayLayout => "InvalidArrayLayout".to_owned(),
            LoweringErrorKind::InvalidStructLayout => "InvalidStructLayout".to_owned(),
            LoweringErrorKind::InvalidStructField(index) => {
                format!("InvalidStructField({index})")
            }
            LoweringErrorKind::InvalidEnumLayout => "InvalidEnumLayout".to_owned(),
            LoweringErrorKind::InvalidEnumVariant(index) => {
                format!("InvalidEnumVariant({index})")
            }
            LoweringErrorKind::InvalidMatchArms => "InvalidMatchArms".to_owned(),
            LoweringErrorKind::NonExhaustiveMatch => "NonExhaustiveMatch".to_owned(),
            LoweringErrorKind::InvalidBlockExpression => "InvalidBlockExpression".to_owned(),
            LoweringErrorKind::InvalidRangeFor => "InvalidRangeFor".to_owned(),
        }
    }
}

impl std::fmt::Display for LoweringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Without a db the unit path/construct/range are unavailable; still emit #gN:nN.
        // Prefer [`Self::display_with_db`] at codegen boundaries.
        write!(f, "{} at #{}", self.kind_label(), self.key.cursor_label())
    }
}

impl std::fmt::Debug for LoweringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}


#[derive(Debug)]
pub enum FunctionEmissionError {
    Lowering(LoweringError),
    Verification { site: AstNodeKey, message: String },
}

impl std::fmt::Display for FunctionEmissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Lowering(error) => write!(f, "Lowering({error})"),
            Self::Verification { site, message } => {
                write!(f, "Verification({message} at #{})", site.cursor_label())
            }
        }
    }
}

impl FunctionEmissionError {
    pub fn verification(site: AstNodeKey, message: impl Into<String>) -> Self {
        Self::Verification { site, message: message.into() }
    }

    /// Expand nested AstNodeKey labels with path, construct, and source range when a db is available.
    pub fn display_with_db(&self, db: &dyn beskid_queries::Db) -> String {
        match self {
            Self::Lowering(error) => format!("Lowering({})", error.display_with_db(db)),
            Self::Verification { site, message } => {
                format!("Verification({message} at {})", beskid_queries::format_ast_node_site(db, *site))
            }
        }
    }
}
