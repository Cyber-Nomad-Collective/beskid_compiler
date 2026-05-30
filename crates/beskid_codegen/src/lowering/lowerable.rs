//! [`Lowerable`] implementations and [`lower_program`] driver.

use crate::linking::{FunctionDefIndex, LinkPlan, LinkSymbol};
use crate::lowering::cast_intent::validate_cast_intents;
use crate::lowering::context::{CodegenArtifact, CodegenContext, CodegenResult, ExternImport};
use crate::lowering::expressions::export::{collect_exports, export_linker_name};
use crate::lowering::function::{lower_function, lower_function_with_name, lower_method, lower_test};
use beskid_analysis::hir::{
    HirContractDefinition, HirContractNode, HirFunctionDefinition, HirInlineModule, HirItem,
    HirProgram, HirTestDefinition,
};
use beskid_analysis::projects::assembly::ProgramAssembly;
use beskid_analysis::resolve::{ItemId, Resolution};
use beskid_analysis::syntax::Spanned;
use beskid_analysis::types::{TypeId, TypeInfo, TypeResult};
use std::collections::HashMap;

/// HIR (or sub-) node that can be lowered with a specific context type `Ctx`.
pub trait Lowerable<Ctx>: Sized {
    type Output;

    fn lower(node: &Spanned<Self>, ctx: &mut Ctx) -> CodegenResult<Self::Output>;
}

/// Dispatch to [`Lowerable::lower`] for the concrete node type behind `T`.
pub fn lower_node<T, Ctx>(node: &Spanned<T>, ctx: &mut Ctx) -> CodegenResult<T::Output>
where
    T: Lowerable<Ctx>,
{
    T::lower(node, ctx)
}

/// Lower an entire [`HirProgram`]: validates cast intents, precomputes named type descriptors, lowers items, collects extern imports.
pub fn lower_program(
    program: &Spanned<HirProgram>,
    resolution: &Resolution,
    type_result: &TypeResult,
) -> Result<CodegenArtifact, Vec<crate::errors::CodegenError>> {
    lower_program_with_assembly(program, resolution, type_result, None)
}

/// Like [`lower_program`], using assembly HIR cache and a reachability-based link plan.
pub fn lower_program_with_assembly(
    program: &Spanned<HirProgram>,
    resolution: &Resolution,
    type_result: &TypeResult,
    assembly: Option<&ProgramAssembly>,
) -> Result<CodegenArtifact, Vec<crate::errors::CodegenError>> {
    lower_program_with_assembly_for_entrypoint(program, resolution, type_result, assembly, None)
}

/// Like [`lower_program_with_assembly`], limiting entry symbols to one function or test name.
pub fn lower_program_with_assembly_for_entrypoint(
    program: &Spanned<HirProgram>,
    resolution: &Resolution,
    type_result: &TypeResult,
    assembly: Option<&ProgramAssembly>,
    link_entrypoint: Option<&str>,
) -> Result<CodegenArtifact, Vec<crate::errors::CodegenError>> {
    let mut errors = validate_cast_intents(type_result);
    let mut ctx = CodegenContext::new();

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

    if let Some(assembly) = assembly {
        let def_index = FunctionDefIndex::build(resolution, &assembly.hir_units);
        let plan = if let Some(entrypoint) = link_entrypoint {
            LinkPlan::build_for_entrypoint(
                program,
                entrypoint,
                resolution,
                type_result,
                &def_index,
            )
        } else {
            LinkPlan::build(program, resolution, type_result, &def_index)
        };
        emit_link_plan(
            program,
            &plan,
            resolution,
            type_result,
            &def_index,
            &mut ctx,
            &mut errors,
        );
    } else {
        let mut function_defs: HashMap<ItemId, &Spanned<HirFunctionDefinition>> = HashMap::new();
        collect_function_defs_by_span(&program.node.items, resolution, &mut function_defs);
        lower_function_items(
            &program.node.items,
            resolution,
            type_result,
            &function_defs,
            &mut ctx,
            &mut errors,
        );
    }

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
            exports: collect_exports(&program.node.items),
        })
    } else {
        Err(errors)
    }
}

fn emit_link_plan(
    entry: &Spanned<HirProgram>,
    plan: &LinkPlan,
    resolution: &Resolution,
    type_result: &TypeResult,
    def_index: &FunctionDefIndex<'_>,
    ctx: &mut CodegenContext,
    errors: &mut Vec<crate::errors::CodegenError>,
) {
    let function_defs = def_index.functions();
    for item in plan.function_item_ids() {
        emit_function_item(
            item,
            None,
            resolution,
            type_result,
            function_defs,
            def_index,
            ctx,
            errors,
        );
    }
    for symbol in &plan.callees {
        emit_link_symbol(
            symbol,
            entry,
            resolution,
            type_result,
            function_defs,
            def_index,
            ctx,
            errors,
        );
    }
    for symbol in &plan.entries {
        emit_link_symbol(
            symbol,
            entry,
            resolution,
            type_result,
            function_defs,
            def_index,
            ctx,
            errors,
        );
    }
}

fn emit_function_item(
    item: ItemId,
    mangled: Option<String>,
    resolution: &Resolution,
    type_result: &TypeResult,
    function_defs: &HashMap<ItemId, &Spanned<HirFunctionDefinition>>,
    def_index: &FunctionDefIndex<'_>,
    ctx: &mut CodegenContext,
    errors: &mut Vec<crate::errors::CodegenError>,
) {
    let Some(def) = def_index.function(item) else {
        return;
    };
    if !def.node.generics.is_empty() {
        return;
    }
    let symbol_name = mangled.clone().unwrap_or_else(|| export_linker_name(def));
    if ctx.symbol_emitted(&symbol_name) {
        return;
    }
    ctx.current_source_path = def_index
        .source_path(item)
        .cloned()
        .or_else(|| {
            resolution
                .items
                .get(item.0)
                .and_then(|info| info.source_path.clone())
        });
    let result = if let Some(name) = mangled {
        lower_function_with_name(
            def,
            resolution,
            type_result,
            function_defs,
            ctx,
            Some(name),
            None,
        )
    } else {
        lower_function(def, resolution, type_result, function_defs, ctx)
    };
    if let Err(error) = result {
        errors.push(error);
    }
    ctx.current_source_path = None;
}

fn emit_link_symbol(
    symbol: &LinkSymbol,
    entry: &Spanned<HirProgram>,
    resolution: &Resolution,
    type_result: &TypeResult,
    function_defs: &HashMap<ItemId, &Spanned<HirFunctionDefinition>>,
    def_index: &FunctionDefIndex<'_>,
    ctx: &mut CodegenContext,
    errors: &mut Vec<crate::errors::CodegenError>,
) {
    match symbol {
        LinkSymbol::Function { item, mangled } => {
            emit_function_item(
                *item,
                mangled.clone(),
                resolution,
                type_result,
                function_defs,
                def_index,
                ctx,
                errors,
            );
        }
        LinkSymbol::Method { item, mangled: _ } => {
            let Some(def) = def_index.method(*item) else {
                return;
            };
            if let Err(error) = lower_method(def, resolution, type_result, function_defs, ctx) {
                errors.push(error);
            }
        }
        LinkSymbol::Test { item, name: _ } => {
            let Some(def) = find_test_by_item(entry, *item, resolution) else {
                return;
            };
            if let Err(error) = lower_test(def, resolution, type_result, function_defs, ctx) {
                errors.push(error);
            }
        }
    }
}

fn find_test_by_item<'a>(
    entry: &'a Spanned<HirProgram>,
    item: ItemId,
    resolution: &Resolution,
) -> Option<&'a Spanned<HirTestDefinition>> {
    let info = resolution.items.get(item.0)?;
    for item_node in &entry.node.items {
        if item_node.span == info.span {
            if let HirItem::TestDefinition(def) = &item_node.node {
                return Some(def);
            }
        }
    }
    None
}

fn collect_function_defs_by_span<'a>(
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
                collect_function_defs_by_span(&module.node.items, resolution, function_defs);
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
