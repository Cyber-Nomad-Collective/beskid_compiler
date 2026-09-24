//! Enum pattern targets and match scrutinee layouts.

use super::super::super::*;
use super::super::explicit_local_declaration_type;
use super::*;
use crate::semantic_contract::typing::{managed_reference_kind_for_syntax_type, pattern_binding_fact_in_environment};

pub(in crate::semantic_contract) fn enum_pattern_targets_declaration(
    db: &dyn Db,
    declaration: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> bool {
    let Some((terminal, module_path)) = path.segments.split_last() else {
        return false;
    };
    if !terminal.node.type_args.is_empty() || module_path.iter().any(|segment| !segment.node.type_args.is_empty()) {
        return false;
    }
    let Some(syntax) =
        db.syntax_unit(declaration.unit).filter(|syntax| syntax.generation(db) == declaration.generation)
    else {
        return false;
    };
    let program = syntax.expanded_program(db);
    syntax
        .syntax_index(db)
        .node_at(program, declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::EnumDefinition>())
        .is_some_and(|definition| definition.name.node.name == terminal.node.name.node.name)
}

/// Resolve the intentionally narrow generic-match surface: an unqualified local path whose
/// declaration is a parameter or a `let` with an explicit complex type annotation. Inferred,
/// chained, and computed scrutinees remain unavailable rather than reviving HIR reconstruction.
pub(in crate::semantic_contract) fn enum_match_scrutinee_layout(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    expression: &beskid_analysis::syntax::MatchExpression,
) -> Option<Result<(AstNodeKey, EnumLayoutFact), SemanticError>> {
    if let beskid_analysis::syntax::Expression::EnumConstructor(constructor) = &expression.scrutinee.node {
        let declaration = resolve_type_declaration(db, key, &constructor.node.path.node.type_path.node)?;
        return Some(
            enum_layout(db, declaration)
                .and_then(|layout| layout.ok_or_else(|| SemanticError::unavailable("enum_match")))
                .map(|layout| (declaration, layout)),
        );
    }
    if matches!(expression.scrutinee.node, beskid_analysis::syntax::Expression::Call(_)) {
        let scrutinee = index.direct_child_id(
            program,
            key.node,
            beskid_analysis::syntax_query::DynNodeRef::from(expression.scrutinee.as_ref()),
        )?;
        let call = AstNodeKey { node: normalized_expression_node(index, scrutinee), ..key };
        return Some(enum_layout_for_direct_call_result(db, call));
    }
    let beskid_analysis::syntax::Expression::Path(path) = &expression.scrutinee.node else {
        return None;
    };
    if path.node.path.node.segments.len() == 2 {
        let scrutinee = index.direct_child_id(
            program,
            key.node,
            beskid_analysis::syntax_query::DynNodeRef::from(expression.scrutinee.as_ref()),
        )?;
        let scrutinee = normalized_expression_node(index, scrutinee);
        let access = aggregate_field_access(db, AstNodeKey { node: scrutinee, ..key }).ok().flatten()?;
        let field = access.layout.fields.get(usize::try_from(access.index).ok()?)?;
        let AggregateFieldShape::Nominal(declaration) = field.1 else {
            return None;
        };
        return Some(
            enum_layout(db, declaration)
                .and_then(|layout| layout.ok_or_else(|| SemanticError::unavailable("enum_match")))
                .map(|layout| (declaration, layout)),
        );
    }
    let local = enum_match_scrutinee_local_declaration(program, index, key, expression)?;
    let parent = parent_node(index, local)?;
    if index.kind(parent)? == beskid_analysis::syntax_query::NodeKind::Pattern {
        let binding = match pattern_binding_fact(db, index, key, local)? {
            Ok(binding) => binding,
            Err(error) => return Some(Err(error)),
        };
        let AggregateFieldShape::Nominal(declaration) = binding.payload else {
            return Some(Err(SemanticError::unavailable("enum_match")));
        };
        return Some(
            enum_layout(db, declaration)
                .and_then(|layout| layout.ok_or_else(|| SemanticError::unavailable("enum_match")))
                .map(|layout| (declaration, layout)),
        );
    }
    let path = enum_match_scrutinee_explicit_type_path(program, index, key, expression)?;
    let declaration = resolve_type_declaration(db, key, &path)?;
    Some(instantiated_enum_layout_for_path(db, key, &path).map(|layout| (declaration, layout)))
}

/// Retain the source-level applied type of a local match scrutinee.
///
/// Enum layout intentionally erases source identities down to ABI shapes. Pattern binding also
/// needs the applied type to distinguish a managed array/nominal from an unmanaged native pointer,
/// so this helper is the single syntax-backed seam used by both layout and ownership projection.
fn enum_match_scrutinee_explicit_type_path(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    expression: &beskid_analysis::syntax::MatchExpression,
) -> Option<beskid_analysis::syntax::Path> {
    let local = enum_match_scrutinee_local_declaration(program, index, key, expression)?;
    let parent = parent_node(index, local)?;
    let annotation = match index.kind(parent)? {
        beskid_analysis::syntax_query::NodeKind::Parameter => index
            .node_at(program, parent)?
            .of::<beskid_analysis::syntax::Parameter>()
            .map(|parameter| &parameter.ty.node),
        beskid_analysis::syntax_query::NodeKind::LetStatement => index
            .node_at(program, parent)?
            .of::<beskid_analysis::syntax::LetStatement>()
            .and_then(|statement| statement.type_annotation.as_ref())
            .map(|annotation| &annotation.node),
        _ => None,
    }?;
    let beskid_analysis::syntax::Type::Complex(path) = annotation else {
        return None;
    };
    Some(path.node.clone())
}

fn enum_match_scrutinee_local_declaration(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    expression: &beskid_analysis::syntax::MatchExpression,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    let beskid_analysis::syntax::Expression::Path(path) = &expression.scrutinee.node else {
        return None;
    };
    let [segment] = path.node.path.node.segments.as_slice() else {
        return None;
    };
    if !segment.node.type_args.is_empty() {
        return None;
    }
    resolve_lexical_declaration(program, index, key.node, segment.node.name.node.name.as_str())
}

/// Project an applied generic enum type into the source identities needed by pattern bindings.
///
/// Physical enum layouts collapse arrays, functions, nominals, and native pointers to pointer
/// shapes. This environment preserves the source distinction without adding a second layout path.
/// If an applied argument still names an enclosing generic, only an explicit call specialization
/// may provide its identity and ownership; missing substitutions fail closed.
pub(super) fn enum_match_source_environment(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    expression: &beskid_analysis::syntax::MatchExpression,
    declaration: AstNodeKey,
    enclosing: Option<&[GenericSubstitution]>,
) -> Result<HashMap<String, GenericSourceTypeIdentity>, SemanticError> {
    let syntax = db
        .syntax_unit(declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, declaration))
        .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
    let definition = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::EnumDefinition>())
        .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
    if definition.generics.is_empty() {
        return Ok(HashMap::new());
    }
    if matches!(expression.scrutinee.node, beskid_analysis::syntax::Expression::Call(_)) {
        let scrutinee = index
            .direct_child_id(
                program,
                key.node,
                beskid_analysis::syntax_query::DynNodeRef::from(expression.scrutinee.as_ref()),
            )
            .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
        let call = AstNodeKey { node: normalized_expression_node(index, scrutinee), ..key };
        let GenericSourceTypeIdentity::Nominal { arguments, .. } = generic_source_expression_identity(db, call)? else {
            return Err(SemanticError::unavailable("enum_match"));
        };
        if arguments.len() != definition.generics.len() {
            return Err(SemanticError::unavailable("enum_match"));
        }
        return Ok(definition
            .generics
            .iter()
            .zip(arguments.iter())
            .map(|(parameter, argument)| (parameter.node.name.clone(), argument.clone()))
            .collect());
    }
    let Some(path) = enum_match_scrutinee_explicit_type_path(program, index, key, expression) else {
        // Other supported scrutinee shapes retain their existing conservative field inference.
        // In particular, pointer-shaped generic fields remain unavailable rather than guessed.
        return Ok(HashMap::new());
    };
    let terminal = path.segments.last().ok_or_else(|| SemanticError::unavailable("enum_match"))?;
    if terminal.node.type_args.len() != definition.generics.len() {
        return Err(SemanticError::unavailable("enum_match"));
    }
    definition
        .generics
        .iter()
        .zip(terminal.node.type_args.iter())
        .map(|(parameter, argument)| {
            let substitutions = enclosing
                .unwrap_or_default()
                .iter()
                .map(|binding| (binding.parameter.as_ref(), binding.source_identity()))
                .collect();
            let identity = generic_source_type_identity_with_substitutions(db, key, &argument.node, &substitutions)?;
            Ok((parameter.node.name.clone(), identity))
        })
        .collect()
}

pub(super) fn enum_match_scrutinee_layout_in_environment(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    expression: &beskid_analysis::syntax::MatchExpression,
    enclosing: &[GenericSubstitution],
) -> Result<Option<(AstNodeKey, EnumLayoutFact)>, SemanticError> {
    if let Some(path) = enum_match_scrutinee_explicit_type_path(program, index, key, expression) {
        let declaration = resolve_type_declaration(db, key, &path)
            .ok_or_else(|| SemanticError::unavailable("enum_match_specialization"))?;
        let layout = instantiated_enum_layout_for_path_in_environment(db, key, &path, enclosing)?;
        return Ok(Some((declaration, layout)));
    }

    let local = enum_match_scrutinee_local_declaration(program, index, key, expression)
        .ok_or_else(|| SemanticError::unavailable("enum_match_specialization"))?;
    if index.kind(parent_node(index, local).ok_or_else(|| SemanticError::unavailable("enum_match_specialization"))?)
        != Some(beskid_analysis::syntax_query::NodeKind::Pattern)
    {
        return Ok(None);
    }
    let enclosing = Arc::<[GenericSubstitution]>::from(enclosing);
    let binding = pattern_binding_fact_in_environment(db, index, key, local, Some(&enclosing))
        .transpose()?
        .ok_or_else(|| SemanticError::unavailable("enum_match_specialization"))?;
    let identity =
        binding.source_identity.as_ref().ok_or_else(|| SemanticError::unavailable("enum_match_specialization"))?;
    enum_layout_for_source_identity(db, key, identity).map(Some)
}
