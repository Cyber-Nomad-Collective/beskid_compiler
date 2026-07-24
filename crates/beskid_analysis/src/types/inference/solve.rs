//! Constraint solver for local inference metavariables.

use std::collections::{HashMap, HashSet};

use crate::resolve::ItemId;
use crate::syntax::SpanInfo;
use crate::types::result::TypeError;
use crate::types::{TypeId, TypeInfo, TypeTable};

use super::constraint::{Constraint, ConstraintSet, TypeVar};
use super::generic::infer_generic_args_from_call_types;
use super::unify::{is_numeric, unify_types};
use super::{InferenceResult, TypeEnv};

const MAX_PASSES: usize = 32;

pub fn solve_constraints(
    set: ConstraintSet,
    env: &TypeEnv<'_>,
    fallback_span: SpanInfo,
) -> Result<InferenceResult, Vec<TypeError>> {
    let mut state = SolverState::new(env.table());
    let mut changed = true;
    let mut passes = 0usize;

    while changed && passes < MAX_PASSES {
        changed = false;
        passes += 1;
        for constraint in set.iter() {
            changed |= state.apply(constraint, env);
        }
    }

    if passes >= MAX_PASSES && changed {
        state.errors.push(TypeError::UnknownValueType { span: fallback_span });
    }

    state.check_ambiguity(&set, fallback_span);

    if state.errors.is_empty() { Ok(InferenceResult { bindings: state.bindings }) } else { Err(state.errors) }
}

struct SolverState<'a> {
    table: &'a TypeTable,
    bindings: HashMap<TypeVar, TypeId>,
    errors: Vec<TypeError>,
}

impl<'a> SolverState<'a> {
    fn new(table: &'a TypeTable) -> Self {
        Self { table, bindings: HashMap::new(), errors: Vec::new() }
    }

    fn apply(&mut self, constraint: &Constraint, env: &TypeEnv<'_>) -> bool {
        if !self.errors.is_empty() {
            return false;
        }

        match constraint {
            Constraint::Equal { var, ty, span } => self.bind_var(*var, *ty, *span),
            Constraint::EqualVar { left, right, span } => self.unify_vars(*left, *right, *span),
            Constraint::IsNumeric { var, span, .. } => self.check_numeric(*var, *span),
            Constraint::ApplyGeneric { callee, arg_types, result_vars, span } => {
                self.apply_generic(env, *callee, arg_types, result_vars, *span)
            }
            Constraint::VariantOf { var, enum_item, variant, span } => {
                self.check_variant_of(env, *var, *enum_item, variant, *span)
            }
        }
    }

    fn resolve(&self, var: TypeVar) -> Option<TypeId> {
        self.bindings.get(&var).copied()
    }

    fn bind_var(&mut self, var: TypeVar, ty: TypeId, span: SpanInfo) -> bool {
        if let Some(existing) = self.resolve(var) {
            match unify_types(self.table, existing, ty, span) {
                Ok(unified) => {
                    if unified != existing {
                        self.bindings.insert(var, unified);
                        true
                    } else {
                        false
                    }
                }
                Err(error) => {
                    self.errors.push(error);
                    false
                }
            }
        } else {
            self.bindings.insert(var, ty);
            true
        }
    }

    fn unify_vars(&mut self, left: TypeVar, right: TypeVar, span: SpanInfo) -> bool {
        match (self.resolve(left), self.resolve(right)) {
            (Some(left_id), Some(right_id)) => {
                if left_id == right_id {
                    return false;
                }
                match unify_types(self.table, left_id, right_id, span) {
                    Ok(unified) => {
                        self.bindings.insert(left, unified);
                        self.bindings.insert(right, unified);
                        true
                    }
                    Err(error) => {
                        self.errors.push(error);
                        false
                    }
                }
            }
            (Some(type_id), None) => self.bind_var(right, type_id, span),
            (None, Some(type_id)) => self.bind_var(left, type_id, span),
            (None, None) => false,
        }
    }

    fn check_numeric(&mut self, var: TypeVar, span: SpanInfo) -> bool {
        let Some(type_id) = self.resolve(var) else {
            return false;
        };
        if is_numeric(self.table, type_id) {
            return false;
        }
        self.errors.push(TypeError::InvalidBinaryOp { span });
        false
    }

    fn apply_generic(
        &mut self,
        env: &TypeEnv<'_>,
        callee: ItemId,
        arg_types: &[TypeId],
        result_vars: &[TypeVar],
        span: SpanInfo,
    ) -> bool {
        let Some(generic_items) = env.generic_items() else {
            self.errors.push(TypeError::MissingTypeArguments { span });
            return false;
        };
        let Some(function_signatures) = env.function_signatures() else {
            self.errors.push(TypeError::MissingTypeArguments { span });
            return false;
        };

        let Some(inferred) =
            infer_generic_args_from_call_types(self.table, generic_items, function_signatures, callee, arg_types)
        else {
            self.errors.push(TypeError::MissingTypeArguments { span });
            return false;
        };

        if inferred.len() != result_vars.len() {
            self.errors.push(TypeError::GenericArgumentMismatch {
                span,
                expected: result_vars.len(),
                actual: inferred.len(),
            });
            return false;
        }

        let mut changed = false;
        for (var, type_id) in result_vars.iter().zip(inferred.iter()) {
            changed |= self.bind_var(*var, *type_id, span);
        }
        changed
    }

    fn check_variant_of(
        &mut self,
        env: &TypeEnv<'_>,
        var: TypeVar,
        enum_item: ItemId,
        variant: &str,
        span: SpanInfo,
    ) -> bool {
        let Some(enum_variants) = env.enum_variants() else {
            return false;
        };
        let Some(variants) = enum_variants.get(&enum_item) else {
            self.errors.push(TypeError::UnknownEnumType { span });
            return false;
        };
        if !variants.contains_key(variant) {
            self.errors.push(TypeError::UnknownEnumVariant { span, name: variant.to_string() });
            return false;
        }

        if let Some(bound) = self.resolve(var) {
            return self.verify_enum_binding(bound, enum_item, env, span);
        }

        let Some(enum_type) = env.named_type(enum_item) else {
            self.errors.push(TypeError::UnknownEnumType { span });
            return false;
        };
        self.bind_var(var, enum_type, span)
    }

    fn verify_enum_binding(&mut self, bound: TypeId, enum_item: ItemId, env: &TypeEnv<'_>, span: SpanInfo) -> bool {
        let expected = env.named_type(enum_item);
        match self.table.get(bound) {
            Some(TypeInfo::Named(item)) if *item == enum_item => false,
            Some(TypeInfo::Applied { base, .. }) if *base == enum_item => false,
            _ => {
                self.errors.push(TypeError::TypeMismatch { span, expected: expected.unwrap_or(bound), actual: bound });
                false
            }
        }
    }

    fn check_ambiguity(&mut self, set: &ConstraintSet, fallback_span: SpanInfo) {
        let mut reported: HashSet<TypeVar> = HashSet::new();
        for var in set.must_resolve() {
            if self.resolve(*var).is_some() || !reported.insert(*var) {
                continue;
            }
            let (span, name) = ambiguity_site(set, *var).unwrap_or((fallback_span, var.0.to_string()));
            self.errors.push(TypeError::MissingTypeAnnotation { span, name });
        }
    }
}

fn ambiguity_site(set: &ConstraintSet, var: TypeVar) -> Option<(SpanInfo, String)> {
    for constraint in set.iter() {
        match constraint {
            Constraint::IsNumeric { var: candidate, span, name } if *candidate == var => {
                return Some((*span, name.clone()));
            }
            Constraint::ApplyGeneric { result_vars, span, .. } if result_vars.contains(&var) => {
                return Some((*span, format!("T{}", var.0)));
            }
            _ => {}
        }
    }
    None
}
