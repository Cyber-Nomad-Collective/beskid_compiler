//! Generic call instantiation, specialization, substitution, and identity facts.

use beskid_abi::{abi_v5::AbiType, runtime_source::RuntimeIntrinsicCapability};
use beskid_analysis::projects::ProgramAssembly;
use beskid_analysis::syntax::SyntaxGenerationId;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::db::Db;
use crate::inputs::ProjectSession;

use super::super::queries::{node_kind, node_span};
use super::*;

/// Exact explicit instantiation of a generic source function.
///
/// The invocation keeps its explicit source type-argument syntax. This fact proves that the
/// current generation resolves to one declaration whose generic arity matches those arguments;
/// it never infers a substitution or consults HIR.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct GenericCallInstantiation {
    pub declaration: AstNodeKey,
    pub argument_count: u8,
    /// Concrete ABI identities supplied by explicit terminal or nominal-receiver type arguments.
    /// This remains source syntax; it never consults HIR-derived substitutions.
    pub arguments: Arc<[SemanticTypeId]>,
}

/// One exact ABI specialization selected by a current generic call expression.
///
/// The declaration remains generation-safe and the ABI signature is derived exclusively from
/// this invocation's syntax arguments.  Consumers use both fields as the item identity, so two
/// distinct instantiations cannot accidentally share one module declaration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct GenericCallSpecialization {
    pub declaration: AstNodeKey,
    pub signature: ItemSignature,
    /// Immutable, declaration-ordered bindings used to derive this ABI shape.  Keeping the
    /// environment with the identity is what lets a later body walk substitute `T` in a nested
    /// generic call instead of lowering the declaration once as though `T` were concrete.
    pub substitutions: Arc<[GenericSubstitution]>,
    pub contract_witnesses: Arc<[ContractParameterWitness]>,
}

/// A proven conformance for one callable parameter, not one contract name.
/// These facts exist only during compilation; the argument retains its original value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ContractParameterWitness {
    pub parameter: AstNodeKey,
    pub position: u32,
    pub contract: AstNodeKey,
    pub concrete: AstNodeKey,
    pub(in crate::semantic_contract) source_identity: GenericSourceTypeIdentity,
    pub(in crate::semantic_contract) methods: Arc<[(AstNodeKey, AstNodeKey)]>,
}

/// Exact source-backed application of a generic nominal method receiver.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct GenericNominalMethodReceiver {
    pub method: AstNodeKey,
    pub receiver: AstNodeKey,
    pub owner: AstNodeKey,
    pub substitutions: Arc<[GenericSubstitution]>,
}

/// One concrete binding in a generic specialization environment.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct GenericSubstitution {
    pub parameter: Arc<str>,
    pub argument: SemanticTypeId,
    source_identity: GenericSourceTypeIdentity,
}

/// Canonical source identity retained independently from a type's target ABI representation.
///
/// This is deliberately private to the semantic model. Callers can consume the derived
/// [`SemanticTypeId`] through [`GenericSubstitution::argument`], while specialization identity
/// remains impossible to reconstruct from pointer-shaped ABI facts.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub(in crate::semantic_contract) enum GenericSourceTypeIdentity {
    Abi(SemanticTypeId),
    Nominal { qualified_name: Arc<str>, arguments: Arc<[GenericSourceTypeIdentity]> },
    Array(Box<GenericSourceTypeIdentity>),
    Function { parameters: Arc<[GenericSourceTypeIdentity]>, result: Box<GenericSourceTypeIdentity> },
}

impl GenericSubstitution {
    /// Construct an ABI-inferred substitution when no more specific source identity exists.
    pub fn inferred(parameter: impl Into<Arc<str>>, argument: SemanticTypeId) -> Self {
        Self { parameter: parameter.into(), argument, source_identity: GenericSourceTypeIdentity::Abi(argument) }
    }

    /// Rebind the same source argument to a nested generic declaration parameter.
    pub fn rebind(&self, parameter: impl Into<Arc<str>>) -> Self {
        Self { parameter: parameter.into(), argument: self.argument, source_identity: self.source_identity.clone() }
    }

    /// Whether values supplied by this source-level binding are traced by the GC.
    ///
    /// This preserves the distinction erased by pointer-shaped ABI specialization: arrays,
    /// functions, and nominal values are managed, while an explicit native pointer is not.
    pub fn managed_reference_kind(&self) -> ManagedReferenceKind {
        self.source_identity.managed_reference_kind()
    }

    pub(in crate::semantic_contract) fn from_source(
        parameter: impl Into<Arc<str>>,
        argument: SemanticTypeId,
        source_identity: GenericSourceTypeIdentity,
    ) -> Self {
        Self { parameter: parameter.into(), argument, source_identity }
    }

    pub(in crate::semantic_contract) fn source_identity(&self) -> &GenericSourceTypeIdentity {
        &self.source_identity
    }
}

impl GenericSourceTypeIdentity {
    pub(in crate::semantic_contract) fn abi_type(&self) -> SemanticTypeId {
        match self {
            Self::Abi(argument) => *argument,
            Self::Nominal { .. } | Self::Array(_) | Self::Function { .. } => SemanticTypeId::POINTER,
        }
    }

    pub(in crate::semantic_contract) fn managed_reference_kind(&self) -> ManagedReferenceKind {
        match self {
            Self::Abi(SemanticTypeId::STRING) | Self::Nominal { .. } | Self::Array(_) | Self::Function { .. } => {
                ManagedReferenceKind::GcManaged
            }
            Self::Abi(_) => ManagedReferenceKind::NativeOrScalar,
        }
    }
}

/// Source-owned element type of an indexed array expression before an enclosing generic
/// declaration has been specialized.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ArrayIndexElementTemplate {
    Concrete(SemanticTypeId),
    EnclosingParameter(Arc<str>),
}

/// A fully materializable generic declaration instance.  This is deliberately detached from a
/// call node: module emission may discover the same instance from several callers but declares
/// exactly one item for its `(declaration, substitutions)` identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct GenericSpecializationInstance {
    pub declaration: AstNodeKey,
    /// Deterministic assembly-relative declaration identity used only for emitted symbols.
    pub declaration_identity: Arc<str>,
    pub signature: ItemSignature,
    pub substitutions: Arc<[GenericSubstitution]>,
    pub contract_witnesses: Arc<[ContractParameterWitness]>,
}

/// A nested generic call whose source type arguments refer to the enclosing declaration's
/// generic parameters.  `parameter_arguments` are resolved only while walking a concrete
/// [`GenericSpecializationInstance`]; they are never guessed from an uninstantiated body.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct GenericCallTemplate {
    pub declaration: AstNodeKey,
    /// Declaration-ordered generic parameter names of the nested callee.
    pub parameters: Arc<[Arc<str>]>,
    pub parameter_arguments: Arc<[Arc<str>]>,
}

/// Stable module identity for a materialized generic declaration.
///
/// ABI signatures alone are not sufficient: distinct nominal substitutions may lower to the
/// same pointer ABI. The emitted identity therefore records the deterministic qualified
/// declaration name plus every ordered parameter name and recursive source-type shape; process-
/// local syntax generations and node numbers remain lookup authority only and never enter it.
pub fn generic_specialization_identity(instance: &GenericSpecializationInstance) -> Arc<[u32]> {
    let mut identity = vec![u32::try_from(instance.declaration_identity.len()).unwrap_or(u32::MAX)];
    identity.extend(instance.declaration_identity.bytes().map(u32::from));
    identity.extend([u32::try_from(instance.substitutions.len()).unwrap_or(u32::MAX)]);
    for binding in instance.substitutions.iter() {
        // Encode every UTF-8 byte with a length delimiter. This is deliberately not a hash:
        // distinct source parameter names cannot collide in the module identity.
        identity.push(u32::try_from(binding.parameter.len()).unwrap_or(u32::MAX));
        identity.extend(binding.parameter.bytes().map(u32::from));
        append_generic_source_type_identity(&mut identity, &binding.source_identity);
    }
    identity.push(u32::try_from(instance.contract_witnesses.len()).unwrap_or(u32::MAX));
    for witness in instance.contract_witnesses.iter() {
        identity.push(witness.position);
        append_generic_source_type_identity(&mut identity, &witness.source_identity);
    }
    identity.push(u32::MAX);
    identity.extend(instance.signature.parameters.iter().map(|semantic| semantic.0));
    identity.push(instance.signature.result.0);
    identity.into()
}

fn append_generic_source_type_identity(identity: &mut Vec<u32>, source: &GenericSourceTypeIdentity) {
    match source {
        GenericSourceTypeIdentity::Abi(argument) => identity.extend([0, argument.0]),
        GenericSourceTypeIdentity::Nominal { qualified_name, arguments, .. } => {
            identity.extend([1, u32::try_from(qualified_name.len()).unwrap_or(u32::MAX)]);
            identity.extend(qualified_name.bytes().map(u32::from));
            identity.push(u32::try_from(arguments.len()).unwrap_or(u32::MAX));
            for argument in arguments.iter() {
                append_generic_source_type_identity(identity, argument);
            }
        }
        GenericSourceTypeIdentity::Array(element) => {
            identity.push(2);
            append_generic_source_type_identity(identity, element);
        }
        GenericSourceTypeIdentity::Function { parameters, result } => {
            identity.extend([3, u32::try_from(parameters.len()).unwrap_or(u32::MAX)]);
            for parameter in parameters.iter() {
                append_generic_source_type_identity(identity, parameter);
            }
            append_generic_source_type_identity(identity, result);
        }
    }
}
