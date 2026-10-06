//! Extern profile obligations of one root unit (T0901-T0904), judged once per unit.
//!
//! Every `[Extern(...)]` contract must name `Abi:"C"` (T0901) and a `Library` (T0902). A library
//! that the host manifest declares as a `glue "<library>"` owner block is a Glue import: its
//! methods are held to the single Glue representation authority, `glue_binding` (Glue
//! primitives, arrays, generics and `GlueHandle` nominals; raw pointers rejected), and the
//! unit's `RustOwner` placements to `rust_owner_declarations`. Every other extern is a user C
//! import held to the C user profile exactly as the legacy `validate_extern_method`: primitive
//! parameters and results only, with `char`, `string`, and `unit` rejected by shape and `word`
//! and `never` rejected by the profile's permitted scalars. The canonical public Dynamic unit
//! of the Corelib carries compiler-issued runtime bridges and is exempt from the C profile.
//!
//! The manifest's Glue libraries and the Dynamic-unit exemption are assembly facts the query
//! database does not hold, so this obligation is evaluated by the `check` gate with those
//! inputs instead of being a Salsa fact.

use super::*;
use beskid_analysis::syntax::{Attribute, ContractDefinition, ContractMethodSignature, Expression, Literal, PrimitiveType, Spanned, Type};
use beskid_analysis::syntax_query::DynNodeRef;

/// Evaluate the extern profile of every `[Extern]` contract in the unit owning `root` (the
/// unit's `AstNodeId(0)` key), rendered with the legacy codes.
pub fn extern_profile_findings(
    db: &dyn Db,
    root: AstNodeKey,
    glue_libraries: &[String],
    runtime_plane_unit: bool,
) -> Vec<SemanticFinding> {
    let mut findings = Vec::new();
    let Some(syntax) = db.syntax_unit(root.unit).filter(|syntax| syntax.accepts_key(db, root)) else {
        return findings;
    };
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let mut glue_contract_seen = false;
    for node in index.ids_of_kind(NodeKind::ContractDefinition) {
        let Some(definition) = index.node_at(program, node).and_then(|node| node.of::<ContractDefinition>()) else {
            continue;
        };
        let Some(marker) = definition.attributes.iter().find(|attribute| attribute.node.name.node.name == "Extern")
        else {
            continue;
        };
        let site = AstNodeKey { node, ..root };
        let (abi, library) = extern_attribute_arguments(&marker.node);
        if abi.as_deref() != Some("C") {
            findings.push(SemanticFinding {
                kind: SemanticIssueKind::ExternInvalidAbi { abi },
                site,
                related: Vec::new(),
            });
            continue;
        }
        let Some(library) = library else {
            findings.push(SemanticFinding { kind: SemanticIssueKind::ExternMissingLibrary, site, related: Vec::new() });
            continue;
        };
        if runtime_plane_unit {
            continue;
        }
        let glue = glue_libraries.iter().any(|candidate| *candidate == library);
        glue_contract_seen |= glue;
        let methods = index
            .ids_of_kind(NodeKind::ContractMethodSignature)
            .filter(|method| nearest_ancestor(index, *method, |kind| kind == NodeKind::ContractDefinition) == Some(node))
            .collect::<Vec<_>>();
        for method_node in methods {
            let Some(signature) =
                index.node_at(program, method_node).and_then(|node| node.of::<ContractMethodSignature>())
            else {
                continue;
            };
            let method_key = AstNodeKey { node: method_node, ..root };
            if glue {
                if let Err(error) = glue_binding(db, method_key) {
                    findings.push(SemanticFinding {
                        kind: SemanticIssueKind::GlueBindingRejected {
                            method: signature.name.node.name.clone(),
                            detail: error.to_string(),
                        },
                        site: method_key,
                        related: Vec::new(),
                    });
                }
            } else {
                judge_c_profile_method(program, index, method_key, signature, &mut findings);
            }
        }
    }
    if glue_contract_seen || !glue_libraries.is_empty() {
        match rust_owner_declarations(db, root.unit) {
            Err(error) => findings.push(SemanticFinding {
                kind: SemanticIssueKind::GlueBindingRejected { method: "RustOwner".to_string(), detail: error.to_string() },
                site: root,
                related: Vec::new(),
            }),
            Ok(declarations) => {
                for declaration in declarations.iter() {
                    if !glue_libraries.iter().any(|candidate| *candidate == declaration.library) {
                        findings.push(SemanticFinding {
                            kind: SemanticIssueKind::GlueBindingRejected {
                                method: "RustOwner".to_string(),
                                detail: format!(
                                    "library `{}` has no manifest `glue \"{}\"` owner block",
                                    declaration.library, declaration.library
                                ),
                            },
                            site: declaration.declaration,
                            related: Vec::new(),
                        });
                    }
                }
            }
        }
    }
    findings
}

/// `(Abi, Library)` of an `[Extern(...)]` attribute, decoded exactly as the call resolution
/// authority (`extern_contract_import_for_declaration`) decodes them.
fn extern_attribute_arguments(attribute: &Attribute) -> (Option<String>, Option<String>) {
    let mut abi = None;
    let mut library = None;
    for argument in &attribute.arguments {
        let value = match &argument.node.value.node {
            Expression::Literal(literal) => match &literal.node.literal.node {
                Literal::String(raw) => beskid_analysis::syntax::decode_string_literal_token(raw).ok(),
                _ => None,
            },
            _ => None,
        };
        match argument.node.name.node.name.as_str() {
            "Abi" => abi = value,
            "Library" => library = value,
            _ => {}
        }
    }
    (abi, library)
}

/// The legacy C user profile, parameter by parameter then the result, stopping at the first
/// rejection of the method exactly as `validate_extern_method` does.
fn judge_c_profile_method(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    method: AstNodeKey,
    signature: &ContractMethodSignature,
    findings: &mut Vec<SemanticFinding>,
) {
    let name = signature.name.node.name.clone();
    for parameter in &signature.parameters {
        let type_site = index
            .direct_child_id(program, method.node, DynNodeRef::from(parameter))
            .and_then(|parameter_node| {
                index.direct_child_id(program, parameter_node, DynNodeRef::from(&parameter.node.ty))
            })
            .map_or(method, |node| AstNodeKey { node, ..method });
        let rejection = match &parameter.node.ty.node {
            Type::Primitive(primitive) => match primitive.node {
                PrimitiveType::Char | PrimitiveType::String | PrimitiveType::Unit => {
                    Some((type_site, c_profile_detail(&parameter.node.ty, false)))
                }
                PrimitiveType::Word | PrimitiveType::Never => Some((
                    method,
                    format!("C profile disallows the type-shape of parameter `{}`", parameter.node.name.node.name),
                )),
                _ => None,
            },
            _ => Some((type_site, c_profile_detail(&parameter.node.ty, false))),
        };
        if let Some((site, detail)) = rejection {
            findings.push(SemanticFinding {
                kind: SemanticIssueKind::ExternDisallowedParamType { method: name, detail },
                site,
                related: Vec::new(),
            });
            return;
        }
    }
    let Some(result) = signature.return_type.as_ref() else { return };
    let type_site = index
        .direct_child_id(program, method.node, DynNodeRef::from(result))
        .map_or(method, |node| AstNodeKey { node, ..method });
    let rejection = match &result.node {
        Type::Primitive(primitive) => match primitive.node {
            PrimitiveType::Unit | PrimitiveType::Never => None,
            PrimitiveType::Char | PrimitiveType::String => Some((type_site, c_profile_detail(result, true))),
            PrimitiveType::Word => Some((method, "C profile disallows the return type-shape".to_string())),
            _ => None,
        },
        _ => Some((type_site, c_profile_detail(result, true))),
    };
    if let Some((site, detail)) = rejection {
        findings.push(SemanticFinding {
            kind: SemanticIssueKind::ExternDisallowedReturnType { method: name, detail },
            site,
            related: Vec::new(),
        });
    }
}

/// The legacy `extern_disallowed_detail` text for one rejected surface type.
fn c_profile_detail(ty: &Spanned<Type>, is_return: bool) -> String {
    match &ty.node {
        Type::Primitive(primitive) => match primitive.node {
            PrimitiveType::Char => "char is not permitted at the FFI boundary".to_string(),
            PrimitiveType::String => "string is a GC reference; use CStringView at the FFI boundary".to_string(),
            PrimitiveType::Unit if is_return => "unit is the void-return marker; omit the return type".to_string(),
            PrimitiveType::Unit => "unit is not a valid FFI parameter type".to_string(),
            PrimitiveType::Word => {
                "word (pointer-width unsigned) is not in the C profile permitted scalars; use pointer instead"
                    .to_string()
            }
            _ => "type is not permitted at the FFI boundary".to_string(),
        },
        Type::Array(_) => "array types must use CBuffer or CArrayView at the FFI boundary".to_string(),
        Type::Complex(_) => "only primitive types are permitted at the FFI boundary".to_string(),
        Type::Associated { .. } => "associated types are not permitted at the FFI boundary".to_string(),
        Type::This => "This is not permitted at the FFI boundary".to_string(),
        Type::Function { .. } => "function types are not permitted at the FFI boundary".to_string(),
    }
}
