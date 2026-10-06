use std::path::Path;

use crate::syntax::Node;

/// Parse once for dependency discovery. Repairs are not dependency authority.
pub(crate) fn parse_program_for_discovery(
    path: &Path,
    source: &str,
) -> Result<crate::syntax::Spanned<crate::syntax::Program>, super::AssemblyError> {
    let parsed = crate::services::parse_program_with_source_name_and_diagnostics(&path.display().to_string(), source)
        .map_err(|error| super::AssemblyError::Parse {
        path: path.to_path_buf(),
        message: error
            .downcast_ref::<crate::analysis::diagnostics::MietteReportError>()
            .map(|report| discovery_diagnostic_message(report.diagnostic()))
            .unwrap_or_else(|| error.to_string()),
    })?;
    if parsed.recovered || !parsed.diagnostics.is_empty() {
        return Err(super::AssemblyError::Parse {
            path: path.to_path_buf(),
            message: parsed.diagnostics.iter().map(discovery_diagnostic_message).collect::<Vec<_>>().join("; "),
        });
    }
    Ok(parsed.program)
}

fn discovery_diagnostic_message(diagnostic: &crate::analysis::diagnostics::SemanticDiagnostic) -> String {
    format!(
        "{} [{}; bytes {}..{}]",
        diagnostic.message,
        diagnostic.code.as_deref().unwrap_or("parse"),
        diagnostic.span.offset(),
        diagnostic.span.offset().saturating_add(diagnostic.span.len())
    )
}

pub(crate) fn import_paths_from_program(program: &crate::syntax::Program) -> Vec<String> {
    declaration_paths(program, DeclarationKind::Use)
}

pub(crate) fn module_declaration_paths_from_program(program: &crate::syntax::Program) -> Vec<String> {
    declaration_paths(program, DeclarationKind::Module)
}

enum DeclarationKind {
    Use,
    Module,
}

fn declaration_paths(program: &crate::syntax::Program, kind: DeclarationKind) -> Vec<String> {
    let mut pending = program.items.iter().rev().collect::<Vec<_>>();
    let mut paths = Vec::new();
    while let Some(item) = pending.pop() {
        let path = match (&item.node, &kind) {
            (Node::UseDeclaration(declaration), DeclarationKind::Use) => Some(&declaration.node.path.node),
            (Node::ModuleDeclaration(declaration), DeclarationKind::Module) => Some(&declaration.node.path.node),
            _ => None,
        };
        if let Some(path) = path {
            let path =
                path.segments.iter().map(|segment| segment.node.name.node.name.as_str()).collect::<Vec<_>>().join(".");
            if !path.is_empty() {
                paths.push(path);
            }
        }
        if let Node::InlineModule(module) = &item.node {
            pending.extend(module.node.items.iter().rev());
        }
    }
    paths
}

/// Module candidates come only from actual qualified syntax paths, including generic arguments.
pub(crate) fn module_paths_from_qualified_references(program: &crate::syntax::Program) -> Vec<String> {
    use crate::syntax_query::DynNodeRef;
    use std::collections::BTreeSet;
    let mut paths = BTreeSet::new();
    let mut pending = vec![DynNodeRef::from(program)];
    while let Some(node) = pending.pop() {
        // Declarations have their own exact collectors; aliases are not qualified references.
        if node.of::<crate::syntax::UseDeclaration>().is_some()
            || node.of::<crate::syntax::ModuleDeclaration>().is_some()
        {
            continue;
        }
        // A nominal type can be homonymous with its complete leaf module
        // (Nodes.NodeRef -> Nodes/NodeRef.bd). Prefix-only discovery omits
        // that source unless an unrelated use declaration happens to load it.
        let nominal = match node.of::<crate::syntax::Type>() {
            Some(crate::syntax::Type::Complex(path)) => Some(path),
            Some(crate::syntax::Type::Associated { contract, .. }) => Some(contract),
            _ => None,
        };
        if let Some(path) = nominal {
            let segments = path.node.segments.iter().map(|segment| segment.node.name.node.name.as_str()).collect::<Vec<_>>();
            if segments.len() >= 2
                && segments.first().is_some_and(|name| name.as_bytes().first().is_some_and(u8::is_ascii_uppercase))
            {
                paths.insert(segments.join("."));
            }
        }
        if let Some(path) = node.of::<crate::syntax::Path>() {
            let segments = path.segments.iter().map(|segment| segment.node.name.node.name.as_str()).collect::<Vec<_>>();
            if segments.first().is_some_and(|name| name.as_bytes().first().is_some_and(u8::is_ascii_uppercase)) {
                for length in 1..segments.len() {
                    paths.insert(segments[..length].join("."));
                }
            }
        }
        node.children(|child| pending.push(child));
    }
    paths.into_iter().collect()
}

/// When a unit imports nested symbols (`Core.Syscall.ReadRequest`), also pull in the
/// parent module facade (`Core/Syscall/Syscall.bd`) that hosts sibling functions referenced via
/// qualified paths (`Core.Syscall.ReadWith`) without an explicit `use`.
pub(crate) fn parent_module_import_path(import_path: &str) -> Option<String> {
    let segments: Vec<&str> = import_path.split('.').filter(|segment| !segment.is_empty()).collect();
    if segments.len() <= 2 {
        return None;
    }
    Some(segments[..segments.len() - 1].join("."))
}
