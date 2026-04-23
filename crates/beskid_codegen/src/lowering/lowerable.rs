use crate::lowering::cast_intent::validate_cast_intents;
use crate::lowering::context::{CodegenArtifact, CodegenContext, CodegenResult, ExternImport};
use crate::lowering::function::{lower_function, lower_method, lower_test};
use beskid_analysis::hir::{
    HirContractDefinition, HirContractNode, HirFunctionDefinition, HirInlineModule, HirItem,
    HirProgram,
};
use beskid_analysis::resolve::{ItemId, Resolution};
use beskid_analysis::syntax::Spanned;
use beskid_analysis::types::{TypeId, TypeInfo, TypeResult};
use std::collections::HashMap;

pub trait Lowerable<Ctx>: Sized {
    type Output;

    fn lower(node: &Spanned<Self>, ctx: &mut Ctx) -> CodegenResult<Self::Output>;
}

fn collect_function_defs<'a>(
    items: &'a [Spanned<HirItem>],
    resolution: &Resolution,
    function_defs: &mut HashMap<ItemId, &'a Spanned<HirFunctionDefinition>>,
) {
    for item in items {
        match &item.node {
            HirItem::FunctionDefinition(def) => {
                if let Some(info) = resolution.items.iter().find(|info| info.span == item.span) {
                    function_defs.insert(info.id, def);
                }
            }
            HirItem::InlineModule(module) => {
                collect_function_defs(&module.node.items, resolution, function_defs);
            }
            _ => {}
        }
    }
}

fn lower_function_items(
    items: &[Spanned<HirItem>],
    resolution: &Resolution,
    type_result: &TypeResult,
    function_defs: &HashMap<ItemId, &Spanned<HirFunctionDefinition>>,
    ctx: &mut CodegenContext,
    errors: &mut Vec<crate::errors::CodegenError>,
) {
    for item in items {
        match &item.node {
            HirItem::FunctionDefinition(def) => {
                if def.node.generics.is_empty()
                    && let Err(error) =
                        lower_function(def, resolution, type_result, function_defs, ctx)
                {
                    errors.push(error);
                }
            }
            HirItem::TestDefinition(def) => {
                if let Err(error) = lower_test(def, resolution, type_result, function_defs, ctx) {
                    errors.push(error);
                }
            }
            HirItem::MethodDefinition(def) => {
                if let Err(error) = lower_method(def, resolution, type_result, function_defs, ctx) {
                    errors.push(error);
                }
            }
            HirItem::InlineModule(module) => {
                lower_function_items(
                    &module.node.items,
                    resolution,
                    type_result,
                    function_defs,
                    ctx,
                    errors,
                );
            }
            _ => {}
        }
    }
}

pub fn lower_node<T, Ctx>(node: &Spanned<T>, ctx: &mut Ctx) -> CodegenResult<T::Output>
where
    T: Lowerable<Ctx>,
{
    T::lower(node, ctx)
}

pub fn lower_program(
    program: &Spanned<HirProgram>,
    resolution: &Resolution,
    type_result: &TypeResult,
) -> Result<CodegenArtifact, Vec<crate::errors::CodegenError>> {
    let mut errors = validate_cast_intents(type_result);
    let mut ctx = CodegenContext::new();

    let mut function_defs: HashMap<ItemId, &Spanned<HirFunctionDefinition>> = HashMap::new();
    collect_function_defs(&program.node.items, resolution, &mut function_defs);

    let mut index = 0usize;
    loop {
        let type_id = TypeId(index);
        let Some(info) = type_result.types.get(type_id) else {
            break;
        };
        if matches!(info, TypeInfo::Named(_) | TypeInfo::Applied { .. }) {
            let _ = ctx.type_descriptor(type_result, type_id);
        }
        index += 1;
    }

    lower_function_items(
        &program.node.items,
        resolution,
        type_result,
        &function_defs,
        &mut ctx,
        &mut errors,
    );

    if errors.is_empty() {
        Ok(CodegenArtifact {
            functions: ctx.lowered_functions,
            type_descriptors: ctx.type_descriptors,
            string_literals: ctx.string_literals,
            extern_imports: {
                let mut v = Vec::new();
                collect_extern_imports(&program.node.items, None, &mut v);
                v
            },
        })
    } else {
        Err(errors)
    }
}

fn collect_extern_imports(
    items: &[Spanned<HirItem>],
    parent_extern: Option<beskid_analysis::hir::HirExternInterface>,
    out: &mut Vec<ExternImport>,
) {
    for item in items {
        match &item.node {
            HirItem::InlineModule(m) => {
                let m: &beskid_analysis::syntax::Spanned<HirInlineModule> = m;
                let effective = m
                    .node
                    .extern_interface
                    .clone()
                    .or_else(|| parent_extern.clone());
                // Record function defs inside extern modules as extern imports
                if let Some(ext) = effective.as_ref() {
                    for sub in &m.node.items {
                        if let HirItem::FunctionDefinition(def) = &sub.node {
                            out.push(ExternImport {
                                symbol: def.node.name.node.name.clone(),
                                abi: ext.abi.clone(),
                                library: ext.library.clone(),
                            });
                        }
                    }
                }
                collect_extern_imports(&m.node.items, effective, out);
            }
            HirItem::ContractDefinition(c) => {
                let c: &beskid_analysis::syntax::Spanned<HirContractDefinition> = c;
                if let Some(ext) = c.node.extern_interface.as_ref() {
                    for it in &c.node.items {
                        if let HirContractNode::MethodSignature(sig) = &it.node {
                            out.push(ExternImport {
                                symbol: sig.node.name.node.name.clone(),
                                abi: ext.abi.clone(),
                                library: ext.library.clone(),
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
