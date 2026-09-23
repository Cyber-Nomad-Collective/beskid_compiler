use crate::abi_v5::{AbiManifestV5, TargetMetadata};

use super::abi_shape::{CorelibServiceAbi, corelib_binding_abi, corelib_service_abi_type};
use super::capability::CorelibServiceCapability;
use super::errors::CorelibServiceImportPreflightError;
use super::service_table::CorelibService;

/// Preflight one compiler-owned Corelib declaration before a backend imports its native adapter.
///
/// The check deliberately joins all three authorities that otherwise live at separate seams:
/// source-scoped Corelib capability, the exact canonical target manifest, and the generated
/// target binding table. It accepts no inferred symbol, target fallback, or compatibility alias.
/// That makes this the pre-Glue interop gate for every Corelib native service, including the
/// Networking facade once its source inventory is registered.
pub fn preflight_corelib_service_declaration(
    capability: &CorelibServiceCapability,
    manifest: &AbiManifestV5,
    source_path: &str,
    name: &str,
    symbol: &str,
) -> Result<CorelibServiceAbi, CorelibServiceImportPreflightError> {
    let service = capability.service_for_source(source_path, name).ok_or_else(|| {
        CorelibServiceImportPreflightError::UnauthorizedDeclaration {
            name: name.to_owned(),
            symbol: symbol.to_owned(),
            source_path: source_path.to_owned(),
        }
    })?;
    if service.symbol != symbol {
        return Err(CorelibServiceImportPreflightError::UnauthorizedDeclaration {
            name: name.to_owned(),
            symbol: symbol.to_owned(),
            source_path: source_path.to_owned(),
        });
    }
    preflight_corelib_service_import(capability, manifest, service)
}

/// Preflight an already source-authorized Corelib service fact before lowering its native call.
///
/// Codegen uses this form because semantic facts retain a [`CorelibService`] rather than a
/// re-parsed declaration. It still rechecks the source authority so stale, forged, or otherwise
/// detached facts cannot be turned into a native import.
pub fn preflight_corelib_service_import(
    capability: &CorelibServiceCapability,
    manifest: &AbiManifestV5,
    service: CorelibService,
) -> Result<CorelibServiceAbi, CorelibServiceImportPreflightError> {
    if manifest.validate().is_err() {
        return Err(CorelibServiceImportPreflightError::InvalidManifest);
    }
    let target = manifest.target.triple.as_str().to_owned();
    if manifest != &AbiManifestV5::canonical_runtime(manifest.target.clone()) {
        return Err(CorelibServiceImportPreflightError::NonCanonicalManifest { target });
    }
    if capability.service_for_source(service.source_path, service.name) != Some(service) {
        return Err(CorelibServiceImportPreflightError::UnauthorizedDeclaration {
            name: service.name.to_owned(),
            symbol: service.symbol.to_owned(),
            source_path: service.source_path.to_owned(),
        });
    }

    let bindings = crate::generated::abi_v5_contract::ABI_V5_CORELIB_SERVICE_BINDINGS
        .iter()
        .filter(|binding| binding.service == service.name)
        .collect::<Vec<_>>();
    if bindings.is_empty() {
        return preflight_corelib_source_builtin(service);
    }

    let mut seen_targets = std::collections::BTreeSet::new();
    for binding in &bindings {
        if !seen_targets.insert(binding.target) {
            return Err(CorelibServiceImportPreflightError::DuplicateTargetBinding {
                name: service.name.to_owned(),
                target: binding.target.to_owned(),
            });
        }
    }

    let mut expected_targets = TargetMetadata::supported()
        .into_iter()
        .map(|candidate| candidate.triple.as_str().to_owned())
        .collect::<Vec<_>>();
    expected_targets.sort();
    expected_targets.dedup();
    let mut actual_targets = bindings.iter().map(|binding| binding.target.to_owned()).collect::<Vec<_>>();
    actual_targets.sort();
    actual_targets.dedup();
    if actual_targets != expected_targets {
        return Err(CorelibServiceImportPreflightError::TargetCoverageMismatch {
            name: service.name.to_owned(),
            expected: expected_targets,
            actual: actual_targets,
        });
    }

    let first = bindings[0];
    let expected_abi =
        corelib_binding_abi(first).ok_or_else(|| CorelibServiceImportPreflightError::TargetShapeMismatch {
            name: service.name.to_owned(),
            target: first.target.to_owned(),
        })?;
    for binding in &bindings {
        if binding.adapter != service.symbol {
            return Err(CorelibServiceImportPreflightError::AdapterMismatch {
                name: service.name.to_owned(),
                expected: service.symbol.to_owned(),
                actual: binding.adapter.to_owned(),
            });
        }
        if binding.implementation != service.symbol {
            return Err(CorelibServiceImportPreflightError::ImplementationMismatch {
                name: service.name.to_owned(),
                expected: service.symbol.to_owned(),
                actual: binding.implementation.to_owned(),
                target: binding.target.to_owned(),
            });
        }
        if binding.params != first.params || binding.result != first.result {
            return Err(CorelibServiceImportPreflightError::TargetShapeMismatch {
                name: service.name.to_owned(),
                target: binding.target.to_owned(),
            });
        }
        if corelib_binding_abi(binding).as_ref() != Some(&expected_abi) {
            return Err(CorelibServiceImportPreflightError::TargetShapeMismatch {
                name: service.name.to_owned(),
                target: binding.target.to_owned(),
            });
        }
    }

    let current =
        bindings.iter().filter(|binding| binding.target == manifest.target.triple.as_str()).collect::<Vec<_>>();
    if current.len() != 1 {
        return Err(if current.is_empty() {
            CorelibServiceImportPreflightError::TargetCoverageMismatch {
                name: service.name.to_owned(),
                expected: vec![manifest.target.triple.as_str().to_owned()],
                actual: Vec::new(),
            }
        } else {
            CorelibServiceImportPreflightError::DuplicateTargetBinding {
                name: service.name.to_owned(),
                target: manifest.target.triple.as_str().to_owned(),
            }
        });
    }
    Ok(expected_abi)
}

/// Validate the small target-independent source-builtin class.
///
/// These declarations have no per-target service rows because their generated source declaration
/// is shared verbatim by every canonical ABI-v5 target. They still retain exact source name,
/// symbol, and shape authority; absence or duplication remains a hard failure.
fn preflight_corelib_source_builtin(
    service: CorelibService,
) -> Result<CorelibServiceAbi, CorelibServiceImportPreflightError> {
    let declarations = crate::generated::abi_v5_contract::ABI_V5_SOURCE_BUILTINS
        .iter()
        .filter(|declaration| declaration.name == service.name)
        .collect::<Vec<_>>();
    let [declaration] = declarations.as_slice() else {
        return Err(if declarations.is_empty() {
            CorelibServiceImportPreflightError::MissingManifestService { name: service.name.to_owned() }
        } else {
            CorelibServiceImportPreflightError::DuplicateManifestDeclaration { name: service.name.to_owned() }
        });
    };
    if declaration.symbol != service.symbol {
        return Err(CorelibServiceImportPreflightError::AdapterMismatch {
            name: service.name.to_owned(),
            expected: service.symbol.to_owned(),
            actual: declaration.symbol.to_owned(),
        });
    }
    let Some(parameters) = declaration.params.iter().copied().map(corelib_service_abi_type).collect::<Option<Vec<_>>>()
    else {
        return Err(CorelibServiceImportPreflightError::TargetShapeMismatch {
            name: service.name.to_owned(),
            target: "all supported targets".to_owned(),
        });
    };
    let Some(result) = corelib_service_abi_type(declaration.result) else {
        return Err(CorelibServiceImportPreflightError::TargetShapeMismatch {
            name: service.name.to_owned(),
            target: "all supported targets".to_owned(),
        });
    };
    Ok(CorelibServiceAbi { parameters, result })
}
