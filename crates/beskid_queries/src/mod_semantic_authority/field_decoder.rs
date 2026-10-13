//! Field decoder factory correspondence of retained serialization decoder groups.
//!
//! A generated decoder group carries, besides its bound `Decoder<T>` implementation, an unbound
//! field decoder factory: a function over the target's own generic parameters and one format
//! Reader that returns `Result<T, SerializationError>`. The correspondence selects that function
//! by its resolved signature in the exact current application, never by its name, and admits it
//! only after the group's retained target and contribution rebind in the same environment.
use super::*;
use beskid_analysis::syntax::{FunctionDefinition, Node};

/// The current factory function of one admitted decoder group and its complete environment.
pub(crate) struct IssuedFieldDecoderFactory {
    pub(crate) function: AstNodeKey,
    pub(crate) substitutions: Arc<[crate::GenericSubstitution]>,
}

enum RetainedGroup<'a> {
    Target(&'a CompiledSerializationTarget),
    Template(&'a CompiledSerializationTemplate),
}

/// Select the single admitted field decoder factory of the retained `[Serialize]` target
/// application `qualified_name<arguments>`.
///
/// `environment` binds a candidate function's generic parameters (target parameters, then the
/// reader); `accepts` checks the candidate's resolved signature against the requested field read.
/// A group is admitted only when exactly one of its functions is accepted and its retained target
/// rebinds as the canonical Decoder of the same application in that environment.
pub(crate) fn generated_field_decoder_factory(
    db: &dyn Db,
    program: &crate::TypedProgram,
    qualified_name: &Arc<str>,
    arguments: &Arc<[ShapeIdentity]>,
    environment: &dyn Fn(&FunctionDefinition) -> Option<Vec<crate::GenericSubstitution>>,
    accepts: &dyn Fn(AstNodeKey, &[crate::GenericSubstitution]) -> Result<bool, crate::SemanticError>,
) -> Result<IssuedFieldDecoderFactory, ModSemanticError> {
    let reject = ModSemanticQueryAuthority::error;
    let groups = program
        .assembly
        .compiled_mod_metadata
        .iter()
        .filter_map(|metadata| {
            metadata
                .issuer_payload::<CompiledSerializationTarget>()
                .map(RetainedGroup::Target)
                .or_else(|| metadata.issuer_payload::<CompiledSerializationTemplate>().map(RetainedGroup::Template))
        })
        .collect::<Vec<_>>();
    let mut selected: Option<IssuedFieldDecoderFactory> = None;
    for group in groups {
        let members = match group {
            RetainedGroup::Target(target) => target.members(),
            RetainedGroup::Template(template) => {
                if template.parameters().len() != arguments.len() {
                    continue;
                }
                let owner = template.current_owner(db, program)?;
                if mod_shape_stable_identity(db, owner).as_ref() != Some(qualified_name) {
                    continue;
                }
                template.members()
            }
        };
        let mut accepted = Vec::new();
        for member in members {
            let Node::Function(function) = &member.item.node else { continue };
            if function.node.generics.len() != arguments.len() + 1 {
                continue;
            }
            let Some(substitutions) = environment(&function.node) else { continue };
            let key = current_entry_function(db, program, &function.node)?;
            if accepts(key, &substitutions).map_err(|error| reject(&error.to_string()))? {
                accepted.push(IssuedFieldDecoderFactory { function: key, substitutions: substitutions.into() });
            }
        }
        let candidate = match accepted.len() {
            0 => continue,
            1 => accepted.pop().expect("one accepted factory"),
            _ => return Err(reject("serialization decoder group has more than one field decoder factory")),
        };
        let rebound = match group {
            RetainedGroup::Target(target) => target.rebind_in_environment(db, program, &candidate.substitutions)?,
            RetainedGroup::Template(template) => {
                template.specialize(db, program, arguments.clone(), &candidate.substitutions)?
            }
        };
        if rebound.kind() != SerializationContributionKind::Decoder
            || mod_shape_stable_identity(db, rebound.owner()).as_ref() != Some(qualified_name)
            || rebound.arguments() != arguments.as_ref()
        {
            return Err(reject("field decoder factory group does not decode the requested target application"));
        }
        if selected.replace(candidate).is_some() {
            return Err(reject("serialization target application has more than one field decoder factory"));
        }
    }
    selected.ok_or_else(|| reject("serialization target application has no admitted field decoder factory"))
}

/// The current entry node of an exactly appended group function. The merger appends generated
/// items to the entry unit; the retained item must occur there exactly once.
fn current_entry_function(
    db: &dyn Db,
    program: &crate::TypedProgram,
    expected: &FunctionDefinition,
) -> Result<AstNodeKey, ModSemanticError> {
    let reject = ModSemanticQueryAuthority::error;
    let entry = program.assembly.entry_unit();
    let syntax = db
        .syntax_unit(SourceUnitId::new(db, entry.path.clone()))
        .ok_or_else(|| reject("serialization contribution entry is not registered"))?;
    let index = syntax.syntax_index(db);
    let matches = index
        .ids_of_kind(NodeKind::FunctionDefinition)
        .filter(|node| {
            index.node_at(&entry.program, *node).and_then(|item| item.of::<FunctionDefinition>()) == Some(expected)
        })
        .map(|node| AstNodeKey { unit: syntax.unit(db), generation: program.generation, node })
        .collect::<Vec<_>>();
    let [function] = matches.as_slice() else {
        return Err(reject("field decoder factory is not appended exactly once"));
    };
    Ok(*function)
}
