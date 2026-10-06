//! Caller-visible function signatures issued through canonical name resolution.
use super::*;
use crate::semantic_contract::mod_shapes::{
    mod_function_accessible, mod_function_signature_projection, mod_resolve_function_route,
};

const ROUTE_LIMIT: usize = 64;
const SEGMENT_LIMIT: usize = 256;

impl ModSemanticQueryAuthority<'_> {
    /// Resolve `path` from an issued caller reference. Parameter and result types are issued by
    /// the same handle table as `type_shape`, so equal applications share one token.
    pub(super) fn resolve_function_signature(
        &self,
        invocation: u64,
        reference: &ModSyntaxNodeRef,
        path: &[String],
    ) -> Result<ModSemanticFunctionSignature, ModSemanticError> {
        self.validate_registration()?;
        let caller = self.syntax_key(invocation, reference)?;
        if path.is_empty()
            || path.len() > ROUTE_LIMIT
            || path.iter().any(|segment| segment.is_empty() || segment.len() > SEGMENT_LIMIT)
        {
            return Err(Self::error("function route exceeds bounds"));
        }
        let declaration = mod_resolve_function_route(self.db, caller, path)
            .ok_or_else(|| Self::error("function route does not resolve to one caller-visible declaration"))?;
        if declaration.generation != self.assembly.generation
            || self.registered_unit(declaration.unit.path(self.db)).is_none()
        {
            return Err(Self::error("resolved function is outside the registered assembly generation"));
        }
        if !mod_function_accessible(self.db, caller, declaration) {
            return Err(Self::error("resolved function is not visible from the caller"));
        }
        let projection =
            mod_function_signature_projection(self.db, declaration).map_err(|error| Self::error(&error.to_string()))?;
        let mut remaining = HANDLE_LIMIT;
        let parameters = projection
            .parameters
            .iter()
            .map(|identity| self.field_type(identity, 0, &mut remaining))
            .collect::<Result<Vec<_>, _>>()?;
        let result = self.field_type(&projection.result, 0, &mut remaining)?;
        Ok(ModSemanticFunctionSignature {
            declaration: self.declaration(declaration)?,
            generic_count: projection.generic_count,
            parameters,
            result,
        })
    }
}
