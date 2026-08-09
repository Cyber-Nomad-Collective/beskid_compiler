use std::collections::HashMap;

use beskid_analysis::{
    hir::HirFunctionDefinition,
    resolve::{ItemId, Resolution, canonical_item_id},
    syntax::Spanned,
    types::TypeResult,
};

use crate::lowering::context::{CodegenContext, CodegenResult};

use super::{
    body_emission::{finish_emitting, lower_function_with_name_body},
    return_types::item_id_for_item_span,
};

pub(crate) fn lower_function(
    def: &Spanned<HirFunctionDefinition>,
    resolution: &Resolution,
    type_result: &TypeResult,
    function_defs: &HashMap<ItemId, &Spanned<HirFunctionDefinition>>,
    ctx: &mut CodegenContext,
) -> CodegenResult<()> {
    let saved_source_path = ctx.current_source_path.clone();
    if ctx.current_source_path.is_none() {
        ctx.current_source_path =
            resolution.items.iter().find(|info| info.span == def.span).and_then(|info| info.source_path.clone());
    }
    let result = lower_function_with_name(def, resolution, type_result, function_defs, ctx, None, None, None);
    ctx.current_source_path = saved_source_path;
    result
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_function_with_name(
    def: &Spanned<HirFunctionDefinition>,
    resolution: &Resolution,
    type_result: &TypeResult,
    function_defs: &HashMap<ItemId, &Spanned<HirFunctionDefinition>>,
    ctx: &mut CodegenContext,
    name_override: Option<String>,
    generic_args: Option<HashMap<String, beskid_analysis::types::TypeId>>,
    known_item_id: Option<ItemId>,
) -> CodegenResult<()> {
    let generic_args = generic_args.unwrap_or_default();
    let item_id = known_item_id
        .or_else(|| item_id_for_item_span(resolution, def.span, ctx.current_source_path.as_ref()))
        .map(|id| canonical_item_id(resolution, id));
    if let Some(id) = item_id {
        ctx.emitting_items.insert(id);
    }
    let saved_substitution = std::mem::take(&mut ctx.active_generic_substitution);
    if !generic_args.is_empty() {
        ctx.active_generic_substitution = generic_args.clone();
    }
    let result = lower_function_with_name_body(
        def,
        resolution,
        type_result,
        function_defs,
        ctx,
        name_override,
        &generic_args,
        item_id,
    );
    ctx.active_generic_substitution = saved_substitution;
    finish_emitting(ctx, item_id);
    result
}
