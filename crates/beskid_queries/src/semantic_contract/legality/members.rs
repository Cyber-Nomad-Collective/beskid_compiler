//! Member and match legality facts (E1211, E1301, E1302, E1307, E1304).
//!
//! Each finding here is a name or an arity that the resolved declaration contradicts. The
//! declaration always comes from the authority lowering itself uses for that construct:
//! `aggregate_literal_declaration` for a struct literal, the receiver resolution of
//! `aggregate_field_access` (`field_access_receiver`) for a field read, the contextual type path
//! and `resolve_type_declaration` of `enum_constructor` for an enum constructor, and
//! `enum_match_scrutinee_layout` for the arms of a `match`. When that authority cannot resolve the
//! declaration, nothing is judged: an unresolved type is E1201's fact, and a receiver or scrutinee
//! shape lowering does not support yet is a compiler gap, not a user error.

use super::*;
use beskid_analysis::syntax::{
    EnumConstructorExpression, EnumDefinition, MatchExpression, Pattern, StructLiteralExpression,
    TypeDefinition,
};

/// Why one member reference contradicts its resolved declaration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MemberReferenceKind {
    /// A struct literal field or a field read that the resolved type does not declare (E1211).
    UnknownStructField { name: Arc<str> },
    /// An enum constructor or a match pattern that names a variant the enum does not declare
    /// (E1301).
    UnknownEnumVariant { enum_name: Arc<str>, variant: Arc<str> },
    /// An enum constructor with a payload count other than its variant's field count (E1302).
    EnumConstructorArity { expected: usize, actual: usize },
    /// A match pattern with a payload count other than its variant's field count (E1307).
    PatternArity { expected: usize, actual: usize },
}

/// One member reference that contradicts its resolved declaration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct MemberReferenceFinding {
    /// The offending struct literal field, field read, enum constructor, or match arm pattern.
    pub site: AstNodeKey,
    pub kind: MemberReferenceKind,
}

/// Report the first member reference in `key` (an item) that its resolved declaration
/// contradicts.
pub fn member_reference_legality(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<MemberReferenceFinding> {
    with_registered_syntax(db, key, member_reference_legality_tracked)
}

#[salsa::tracked(persist)]
fn member_reference_legality_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<MemberReferenceFinding> {
    let finding = with_node(db, syntax, key, |program, index, _node| {
        let mut nodes = Vec::new();
        collect_nodes_of_kind(index, key.node, NodeKind::StructLiteralExpression, &mut nodes);
        collect_nodes_of_kind(index, key.node, NodeKind::MemberExpression, &mut nodes);
        collect_nodes_of_kind(index, key.node, NodeKind::PathExpression, &mut nodes);
        collect_nodes_of_kind(index, key.node, NodeKind::EnumConstructorExpression, &mut nodes);
        collect_nodes_of_kind(index, key.node, NodeKind::MatchExpression, &mut nodes);
        nodes.sort_unstable();
        nodes.into_iter().find_map(|node| {
            let site = AstNodeKey { node, ..key };
            let reference = index.node_at(program, node)?;
            if let Some(literal) = reference.of::<StructLiteralExpression>() {
                return unknown_struct_literal_field(db, program, index, site, literal);
            }
            if let Some(constructor) = reference.of::<EnumConstructorExpression>() {
                return enum_constructor_legality(db, program, index, site, constructor);
            }
            if let Some(expression) = reference.of::<MatchExpression>() {
                return match_pattern_legality(db, program, index, site, expression);
            }
            unknown_field_read(db, program, index, site, reference)
        })
    })?;
    Ok(finding)
}

fn unknown_struct_literal_field(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    literal_key: AstNodeKey,
    literal: &StructLiteralExpression,
) -> Option<MemberReferenceFinding> {
    let declaration = aggregate_literal_declaration(db, literal_key).ok()??;
    let declared = declared_member_names(db, declaration)?;
    let field = literal.fields.iter().find(|field| !declared.fields.contains(field.node.name.node.name.as_str()))?;
    let site = index
        .direct_child_id(program, literal_key.node, beskid_analysis::syntax_query::DynNodeRef::from(field))
        .map_or(literal_key, |node| AstNodeKey { node, ..literal_key });
    Some(MemberReferenceFinding {
        site,
        kind: MemberReferenceKind::UnknownStructField { name: Arc::from(field.node.name.node.name.as_str()) },
    })
}

/// A field read (`call().field`, `this.field`, `local.field`) whose receiver resolves to a
/// nominal type that declares no member of that name. Bare single-segment paths are not judged:
/// they may name a module-level item instead of an implicit-receiver field. A member reference
/// that is itself a call's callee is a method call, which `call_lowering` owns.
fn unknown_field_read(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    site: AstNodeKey,
    reference: beskid_analysis::syntax_query::DynNodeRef<'_>,
) -> Option<MemberReferenceFinding> {
    if let Some(path) = reference.of::<beskid_analysis::syntax::PathExpression>()
        && path.path.node.segments.len() != 2
    {
        return None;
    }
    if is_call_callee(program, index, site.node) || nominal_field_projection(db, site).is_some() {
        return None;
    }
    let FieldAccessReceiver { declaration, layout, field_name, .. } =
        field_access_receiver(db, program, index, site, reference, None, None)?.ok()?;
    if layout.fields.iter().any(|(name, _)| name.as_ref() == field_name) {
        return None;
    }
    let declared = declared_member_names(db, declaration)?;
    if declared.fields.contains(field_name)
        || declared.methods.contains(field_name)
        || unique_nominal_method_declaration(db, declaration, field_name).is_some()
    {
        return None;
    }
    Some(MemberReferenceFinding { site, kind: MemberReferenceKind::UnknownStructField { name: Arc::from(field_name) } })
}

fn is_call_callee(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    node: beskid_analysis::syntax::AstNodeId,
) -> bool {
    let mut current = node;
    while let Some(parent) = parent_node(index, current) {
        match index.kind(parent) {
            Some(NodeKind::Expression | NodeKind::GroupedExpression) => current = parent,
            Some(NodeKind::CallExpression) => {
                return index
                    .node_at(program, parent)
                    .and_then(|call| call.of::<beskid_analysis::syntax::CallExpression>())
                    .and_then(|call| {
                        index.direct_child_id(
                            program,
                            parent,
                            beskid_analysis::syntax_query::DynNodeRef::from(call.callee.as_ref()),
                        )
                    })
                    .is_some_and(|callee| callee == current || normalized_expression_node(index, callee) == node);
            }
            _ => return false,
        }
    }
    false
}

struct DeclaredMembers {
    fields: HashSet<String>,
    methods: HashSet<String>,
}

fn declared_member_names(db: &dyn Db, declaration: AstNodeKey) -> Option<DeclaredMembers> {
    let syntax = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration))?;
    let definition = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), declaration.node)?
        .of::<TypeDefinition>()?
        .clone();
    Some(DeclaredMembers {
        fields: definition.fields.iter().map(|field| field.node.name.node.name.clone()).collect(),
        methods: definition.methods.iter().map(|method| method.node.name.node.name.clone()).collect(),
    })
}

fn enum_definition(db: &dyn Db, declaration: AstNodeKey) -> Option<EnumDefinition> {
    let syntax = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration))?;
    syntax.syntax_index(db).node_at(syntax.expanded_program(db), declaration.node)?.of::<EnumDefinition>().cloned()
}

/// The declaration `enum_constructor` resolves: the contextual applied type path when the value
/// context proves one, otherwise the constructor's own type path.
fn enum_constructor_legality(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    site: AstNodeKey,
    constructor: &EnumConstructorExpression,
) -> Option<MemberReferenceFinding> {
    let type_path = contextual_enum_constructor_type_path(db, program, index, site, constructor)
        .unwrap_or_else(|| constructor.path.node.type_path.node.clone());
    let declaration = resolve_type_declaration(db, site, &type_path)?;
    let definition = enum_definition(db, declaration)?;
    let variant_name = constructor.path.node.variant.node.name.as_str();
    let Some(variant) = definition.variants.iter().find(|variant| variant.node.name.node.name == variant_name) else {
        return Some(MemberReferenceFinding {
            site,
            kind: MemberReferenceKind::UnknownEnumVariant {
                enum_name: Arc::from(definition.name.node.name.as_str()),
                variant: Arc::from(variant_name),
            },
        });
    };
    (variant.node.fields.len() != constructor.args.len()).then(|| MemberReferenceFinding {
        site,
        kind: MemberReferenceKind::EnumConstructorArity {
            expected: variant.node.fields.len(),
            actual: constructor.args.len(),
        },
    })
}

/// Top-level enum patterns of a `match` whose scrutinee resolves to an enum declaration: every
/// pattern that names that enum must name one of its variants with that variant's field count.
fn match_pattern_legality(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    site: AstNodeKey,
    expression: &MatchExpression,
) -> Option<MemberReferenceFinding> {
    let (declaration, _) = enum_match_scrutinee_layout(db, program, index, site, expression)?.ok()?;
    let definition = enum_definition(db, declaration)?;
    expression.arms.iter().find_map(|arm| {
        let Pattern::Enum(pattern) = &arm.node.pattern.node else { return None };
        if !enum_pattern_targets_declaration(db, declaration, &pattern.node.path.node.type_path.node) {
            return None;
        }
        let pattern_site = arm_pattern_site(program, index, site, arm);
        let variant_name = pattern.node.path.node.variant.node.name.as_str();
        let Some(variant) = definition.variants.iter().find(|variant| variant.node.name.node.name == variant_name)
        else {
            return Some(MemberReferenceFinding {
                site: pattern_site,
                kind: MemberReferenceKind::UnknownEnumVariant {
                    enum_name: Arc::from(definition.name.node.name.as_str()),
                    variant: Arc::from(variant_name),
                },
            });
        };
        (variant.node.fields.len() != pattern.node.items.len()).then(|| MemberReferenceFinding {
            site: pattern_site,
            kind: MemberReferenceKind::PatternArity {
                expected: variant.node.fields.len(),
                actual: pattern.node.items.len(),
            },
        })
    })
}

fn arm_pattern_site(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    match_key: AstNodeKey,
    arm: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::MatchArm>,
) -> AstNodeKey {
    index
        .direct_child_id(program, match_key.node, beskid_analysis::syntax_query::DynNodeRef::from(arm))
        .and_then(|arm_node| {
            index
                .direct_child_id(program, arm_node, beskid_analysis::syntax_query::DynNodeRef::from(&arm.node.pattern))
                .or(Some(arm_node))
        })
        .map_or(match_key, |node| AstNodeKey { node, ..match_key })
}

/// A `match` over an enum that leaves at least one variant without an arm.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct NonExhaustiveMatch {
    /// The match expression (the diagnostic site).
    pub site: AstNodeKey,
    pub enum_name: Arc<str>,
    /// The first declared variant no unguarded arm names.
    pub missing_variant: Arc<str>,
}

/// Report the first `match` in `key` (an item) whose scrutinee resolves to an enum and that
/// leaves a declared variant without any unguarded arm, with no unguarded wildcard or binding
/// arm (E1304).
///
/// A variant counts as covered as soon as one unguarded arm names it, whatever its payload
/// patterns: nested payload exhaustiveness stays with ISLE's `emit_match_dispatch`. This fact can
/// therefore miss a non-exhaustive match but never rejects one lowering accepts.
pub fn match_exhaustiveness(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<NonExhaustiveMatch> {
    with_registered_syntax(db, key, match_exhaustiveness_tracked)
}

#[salsa::tracked(persist)]
fn match_exhaustiveness_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<NonExhaustiveMatch> {
    let finding = with_node(db, syntax, key, |program, index, _node| {
        let mut matches = Vec::new();
        collect_nodes_of_kind(index, key.node, NodeKind::MatchExpression, &mut matches);
        matches.into_iter().find_map(|node| {
            let site = AstNodeKey { node, ..key };
            let expression = index.node_at(program, node)?.of::<MatchExpression>()?;
            non_exhaustive_match(db, program, index, site, expression)
        })
    })?;
    Ok(finding)
}

fn non_exhaustive_match(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    site: AstNodeKey,
    expression: &MatchExpression,
) -> Option<NonExhaustiveMatch> {
    let unguarded = expression.arms.iter().filter(|arm| arm.node.guard.is_none()).collect::<Vec<_>>();
    if unguarded.iter().any(|arm| matches!(arm.node.pattern.node, Pattern::Wildcard | Pattern::Identifier(_))) {
        return None;
    }
    let (declaration, _) = enum_match_scrutinee_layout(db, program, index, site, expression)?.ok()?;
    let definition = enum_definition(db, declaration)?;
    let covered = unguarded
        .iter()
        .filter_map(|arm| match &arm.node.pattern.node {
            Pattern::Enum(pattern)
                if enum_pattern_targets_declaration(db, declaration, &pattern.node.path.node.type_path.node) =>
            {
                Some(pattern.node.path.node.variant.node.name.as_str())
            }
            _ => None,
        })
        .collect::<HashSet<_>>();
    // A match that names anything but this enum's own variants is a type error with its own
    // diagnostic; do not report it as non-exhaustive as well.
    if unguarded.iter().any(|arm| !matches!(arm.node.pattern.node, Pattern::Enum(_))) {
        return None;
    }
    let missing = definition
        .variants
        .iter()
        .find(|variant| !covered.contains(variant.node.name.node.name.as_str()))?;
    Some(NonExhaustiveMatch {
        site,
        enum_name: Arc::from(definition.name.node.name.as_str()),
        missing_variant: Arc::from(missing.node.name.node.name.as_str()),
    })
}
