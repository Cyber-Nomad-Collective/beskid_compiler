use crate::resolve::{ItemId, ResolvedType};
use crate::syntax::Spanned;
use crate::syntax::Type;
use crate::types::TypeId;
use crate::types::path_value::{generic_mapping_for_type_id, named_item_id};
use crate::types::result::FunctionSignature;

use super::super::substitution::{
    find_applied_type, find_named_type, lookup_function_type, primitive_type_id, substitute_type_id,
};
use super::state::PrepWalker;

impl<'a> PrepWalker<'a> {
    pub(super) fn method_item_for_receiver(
        &self,
        receiver_type: TypeId,
        method_name: &str,
    ) -> Option<ItemId> {
        let receiver_item = named_item_id(&self.surfaces.path_env(), receiver_type)?;
        self.surfaces
            .methods_by_receiver
            .get(&(receiver_item, method_name.to_string()))
            .copied()
            .map(|item| crate::resolve::canonical_item_id(self.resolution, item))
    }

    pub(super) fn method_dispatch_signature(
        &self,
        method_item_id: ItemId,
        receiver_type: TypeId,
    ) -> Option<FunctionSignature> {
        let signature = self
            .surfaces
            .method_function_signatures
            .get(&method_item_id)
            .or_else(|| self.surfaces.function_signatures.get(&method_item_id))?
            .clone();
        let mapping = generic_mapping_for_type_id(&self.surfaces.path_env(), receiver_type);
        if mapping.is_empty() {
            return Some(signature);
        }
        Some(FunctionSignature {
            params: signature.params.iter().map(|p| substitute_type_id(self.surfaces, *p, &mapping)).collect(),
            return_type: substitute_type_id(self.surfaces, signature.return_type, &mapping),
        })
    }

    pub(super) fn named_type_id(&self, item_id: ItemId) -> Option<TypeId> {
        self.surfaces.named_types.get(&item_id).copied().or_else(|| find_named_type(self.surfaces.types, item_id))
    }

    pub(super) fn type_id_for_program_type(&self, ty: &Spanned<Type>) -> Option<TypeId> {
        match &ty.node {
            Type::Primitive(p) => primitive_type_id(self.surfaces.types, p.node),
            Type::Complex(path) => {
                if path.node.segments.len() == 1
                    && path.node.segments[0].node.type_args.is_empty()
                    && let Some(id) = self.generic_params.get(&path.node.segments[0].node.name.node.name)
                {
                    return Some(*id);
                }
                self.type_id_for_type_path(path)
            }
            Type::Associated { .. } => None,
            Type::Array(inner) => {
                let inner_id = self.type_id_for_program_type(inner)?;
                self.surfaces.types.find_array_of(inner_id)
            }
            Type::Function { return_type, parameters } => {
                let ret = self.type_id_for_program_type(return_type)?;
                let params = parameters.iter().map(|p| self.type_id_for_program_type(p)).collect::<Option<Vec<_>>>()?;
                lookup_function_type(self.surfaces.types, &params, ret)
            }
        }
    }

    pub(super) fn type_id_for_type_path(
        &self,
        path: &Spanned<crate::syntax::Path>,
    ) -> Option<TypeId> {
        let ResolvedType::Item(item_id) =
            self.resolution.tables.resolved_type_at(path.span, self.current_source_path.as_ref())?
        else {
            return None;
        };
        let item_id = crate::resolve::canonical_item_id(self.resolution, item_id);
        let base = self.named_type_id(item_id)?;
        let last = path.node.segments.last()?;
        if last.node.type_args.is_empty() {
            return Some(base);
        }
        let args = last.node.type_args.iter().map(|a| self.type_id_for_program_type(a)).collect::<Option<Vec<_>>>()?;
        find_applied_type(self.surfaces.types, item_id, &args)
    }
}
