//! Browser WebAssembly output from the existing syntax-to-CLIF pipeline.
mod arena;
mod binaryen;
mod closures;
mod composition;
mod fibers;
mod scheduler;
mod coverage;
mod diagnostics;
pub mod browser_heap;
mod foundation;
mod host_profile;
pub mod managed_clif;
mod math;
mod network;
mod output;
mod protocol;
pub mod root_liveness;
mod scalar_ops;
mod vector_shifts;
mod strings;
mod suspension;
mod source_map;
mod stdio;
mod text_services;

use anyhow::{Context as _, Result, ensure};
use beskid_codegen::{
    CodegenArtifact,
    cranelift_host::{declare_user_functions_with_link_symbols_and_linkage, remap_testcase_externals},
};
use clif2wasm::WasmModule;
use cranelift_codegen::{
    Context,
    ir::{ExternalName, Function},
    verifier::verify_function,
};
use cranelift_module::{Linkage, Module};
use std::{collections::HashMap, fs, path::Path, process::Command};



fn is_clock_import(name: &str) -> bool {
    matches!(
        name,
        beskid_abi::SYM_CLOCK_MONOTONIC_NANOS | beskid_abi::SYM_CLOCK_REALTIME_NANOS | beskid_abi::SYM_FIBER_NOW_MILLIS
    )
}

pub fn scope_handles(function: &mut Function) {
    use cranelift_codegen::{
        cursor::{Cursor, FuncCursor},
        ir::{ExtFuncData, InstBuilder, Opcode, Signature},
    };
    let signature = Signature::new(function.signature.call_conv);
    let signature = function.import_signature(signature);
    let reset = function.import_function(ExtFuncData {
        name: ExternalName::testcase(browser_heap::RESET_HANDLES),
        signature,
        colocated: false,
        patchable: false,
    });
    let entry = function.layout.entry_block().expect("verified function has an entry");
    let returns: Vec<_> = function
        .layout
        .blocks()
        .filter_map(|b| function.layout.last_inst(b))
        .filter(|&i| function.dfg.insts[i].opcode() == Opcode::Return)
        .collect();
    let mut cursor = FuncCursor::new(function);
    cursor.goto_first_insertion_point(entry);
    cursor.ins().call(reset, &[]);
    for inst in returns {
        cursor.goto_inst(inst);
        cursor.ins().call(reset, &[]);
    }
}

fn rename_export(bytes: &[u8], link_name: &str, alias: &str) -> Result<Vec<u8>> {
    use wasm_encoder::{ExportKind, ExportSection, Module, RawSection};
    use wasmparser::{ExternalKind, Parser, Payload};

    let mut module = Module::new();
    let mut renamed = false;
    for payload in Parser::new(0).parse_all(bytes) {
        let payload = payload?;
        if let Payload::ExportSection(section) = payload {
            let mut exports = ExportSection::new();
            for export in section {
                let export = export?;
                let kind = match export.kind {
                    ExternalKind::Func => ExportKind::Func,
                    ExternalKind::Table => ExportKind::Table,
                    ExternalKind::Memory => ExportKind::Memory,
                    ExternalKind::Global => ExportKind::Global,
                    ExternalKind::Tag => ExportKind::Tag,
                    ExternalKind::FuncExact => anyhow::bail!("unexpected exact function export {}", export.name),
                };
                ensure!(export.name != alias, "duplicate browser export {alias}");
                let name = if export.name == link_name {
                    ensure!(!renamed && kind == ExportKind::Func, "invalid browser entry export {link_name}");
                    renamed = true;
                    alias
                } else {
                    export.name
                };
                exports.export(name, kind, export.index);
            }
            module.section(&exports);
        } else if let Some((id, range)) = payload.as_section() {
            module.section(&RawSection { id, data: &bytes[range] });
        }
    }
    ensure!(renamed, "browser entry export {link_name} is missing");
    Ok(module.finish())
}

pub fn build(mut artifact: CodegenArtifact, entry: &str, path: &Path, optimize: bool, source_map: bool) -> Result<()> {
    closures::retain_reachable(&mut artifact)?;
    closures::validate_indirect(&artifact)?;

    let managed = artifact.contract_boxes_required || artifact.event_handler_wrapper_required || !artifact.array_static_plans.is_empty() || !artifact.aggregate_static_plans.is_empty()
        || artifact.closure_static_plans.iter().any(|plan| !plan.captures.is_empty());
    let mut layout = browser_heap::HeapLayout::default();
    let entry_symbol = artifact.executable_entry.as_deref().context("browser entrypoint missing from codegen artifact")?;
    let mut entries =
        artifact.functions.iter().enumerate().filter(|(_, function)| function.name == entry_symbol);
    let (entry_index, entry_function) = entries.next().context("browser entrypoint missing from CLIF")?;
    ensure!(entries.next().is_none(), "browser entrypoint {entry} is ambiguous");
    let mut links = artifact.browser_exports.clone();
    ensure!(links.get(&entry_function.name).is_none_or(|name| name == entry), "browser entrypoint has a conflicting export name");
    links.insert(entry_function.name.clone(), entry.to_owned());
    // The shared Cranelift module API validates C link names. Keep qualified source aliases
    // at the Wasm boundary, where exports can contain `::`.
    let entry_link = (!beskid_codegen::artifact::is_valid_link_name(entry))
        .then(|| beskid_codegen::artifact::internal_link_symbol(entry_symbol));
    let mut signatures = std::collections::BTreeMap::new();
    for (symbol, export) in &links {
        let signature = artifact.source_signatures.get(symbol)
            .with_context(|| format!("browser export {export} lacks a source signature"))?;
        ensure!(signatures.insert(export.clone(), signature.clone()).is_none(), "duplicate browser export {export}");
    }
    let mut qualification = std::env::var_os(beskid_codegen::artifact::BROWSER_COVERAGE_ENV)
        .is_some_and(|value| value == "1")
        .then(|| coverage::capture(&artifact, &links))
        .transpose()?;
    let original_clif = qualification.as_ref().map(|_| {
        artifact.functions.iter().map(|f| (f.name.clone(), coverage::LoweringSnapshot::capture(&f.function)))
            .collect::<HashMap<_, _>>()
    });
    let spawn_exports: Vec<String> = artifact.spawn_artifacts.iter().enumerate()
        .map(|(index, _)| format!("__beskid_fiber_entry_{}", index + 1)).collect();
    if let Some(name) = &entry_link {
        ensure!(!signatures.contains_key(name) && !spawn_exports.contains(name), "browser entry bridge name conflicts with export {name}");
    }
    for (spawn, export) in artifact.spawn_artifacts.iter().zip(&spawn_exports) {
        ensure!(!links.values().any(|name| name == export), "reserved browser fiber entry name");
        links.insert(spawn.trampoline.clone(), export.clone());
    }
    let composition_enabled = artifact.browser_composition_hooks.is_some();
    let composition_exit = artifact.corelib_service_symbols.contains(composition::EXIT);
    let mut composition_exports = Vec::new();
    for (index, symbol) in artifact.browser_composition_hooks.iter().flatten().enumerate() {
        if artifact.functions.iter().any(|function| &function.name == symbol) {
            let signature = artifact.source_signatures.get(symbol).context("composition hook lacks a source signature")?;
            let export = format!("__beskid_composition_dispose_{}_{}", index + 1, signature.parameters.len());
            ensure!(links.insert(symbol.clone(), export.clone()).is_none(), "composition hook cannot have a public browser export");
            composition_exports.push(export);
        }
    }
    use beskid_queries::{SemanticTypeId as Ty, SourceSignatureType as SourceType};
    let bound_values = signatures.values().flat_map(|signature| signature.parameters.iter().chain(std::iter::once(&signature.result)));
    let copy_strings = bound_values.clone().any(|ty| *ty == SourceType::Scalar(Ty::STRING));
    let network_services: Vec<_> = artifact.functions.iter()
        .flat_map(|function| function.function.dfg.ext_funcs.values())
        .filter_map(|external| match &external.name {
            ExternalName::TestCase(name) => network::service(std::str::from_utf8(name.raw()).ok()?),
            _ => None,
        })
        .collect();
    let copy_words = network_services.iter().any(|service| service.arguments.contains(&network::Argument::ResultHandle));
    let mut internal_exports = spawn_exports.iter().map(String::as_str).collect::<Vec<_>>();
    internal_exports.extend(composition_exports.iter().map(String::as_str));
    if !spawn_exports.is_empty() { internal_exports.extend(arena::FIBER_EXPORTS); }
    let mut string_pool = strings::collect(&mut artifact)?;
    let host_transfers_strings = network_services.iter().any(|service| service.arguments.contains(&network::Argument::String))
        || artifact.functions.iter().any(|function| {
        function.function.dfg.ext_funcs.values().any(|external| {
            matches!(&external.name, ExternalName::TestCase(name) if std::str::from_utf8(name.raw())
                .is_ok_and(|name| name == fibers::JOIN_MESSAGE || text_services::uses_string_transfer(name) || foundation::service(name).is_some_and(|service| service.needs_strings())
                    || beskid_abi::runtime_source::browser_protocol::operation(name).is_some_and(|operation| {
                        use beskid_abi::runtime_source::browser_protocol::Value;
                        operation.result == Value::String || operation.parameters.contains(&Value::String)
                    })))
        })
    });
    let copy_bytes = bound_values.clone().any(host_profile::is_byte_buffer)
        || network_services.iter().any(|service| service.needs_bytes())
        || artifact.functions.iter().any(|function| {
        function.function.dfg.ext_funcs.values().any(|external| {
            matches!(&external.name, ExternalName::TestCase(name) if std::str::from_utf8(name.raw()).is_ok_and(|name|
                beskid_abi::runtime_source::browser_protocol::operation(name).is_some_and(|operation|
                    operation.parameters.contains(&beskid_abi::runtime_source::browser_protocol::Value::Bytes)) || stdio::handles(name)))
        })
    });
    let expose_refs = bound_values.clone().any(host_profile::is_managed);
    let scheduler_transfer = artifact.corelib_service_symbols.iter().any(|name| {
        scheduler::find(name).is_some_and(|service| service.transfer != scheduler::Transfer::None)
    });
    if expose_refs || scheduler_transfer || !spawn_exports.is_empty() || composition_enabled || composition_exit {
        internal_exports.extend(arena::REFERENCE_EXPORTS);
    }
    if copy_bytes {
        internal_exports.extend(host_profile::BYTE_HELPERS);
    }
    if copy_words {
        internal_exports.extend(host_profile::WORD_HELPERS);
    }
    if copy_strings || host_transfers_strings {
        string_pool.enable_host_marshalling();
        internal_exports.extend(strings::HOST_STRING_HELPERS);
    }
    let needs_scope = managed || string_pool.is_active() || expose_refs || copy_bytes || !network_services.is_empty()
        || scheduler_transfer || composition_enabled || composition_exit;
    if needs_scope {
        internal_exports.extend(arena::EXPORTS);
        ensure!(signatures.keys().all(|name| !internal_exports.contains(&name.as_str())), "reserved browser export name");
        artifact.functions.swap(0, entry_index);
        layout = managed_clif::adapt_with_strings(&mut artifact, &string_pool)
            .map_err(|e| anyhow::anyhow!("browser managed lowering: {e}"))?;
        if copy_bytes {
            layout.arrays.insert(browser_heap::ArrayElement::Scalar(browser_heap::Scalar::I8));
        }
        if copy_words {
            layout.arrays.insert(browser_heap::ArrayElement::Scalar(browser_heap::Scalar::I64));
        }
        root_liveness::release_transient_values(&mut artifact, &layout)?;
    }
    let fiber_frames = fibers::instrument(&mut artifact)?;
    for lowered in &mut artifact.functions {
        scalar_ops::expand_bitselect(&mut lowered.function);
        vector_shifts::expand(&mut lowered.function);
    }
    if let (Some(evidence), Some(original)) = (&mut qualification, original_clif) {
        let reuse = artifact.functions.iter().filter_map(|lowered| {
            original.get(&lowered.name).map(|before| (lowered.name.clone(), before.compare(&lowered.function)))
        })
            .collect::<serde_json::Map<_, _>>();
        evidence["generatedFunctions"] = (artifact.functions.len() - reuse.len()).into();
        evidence["clifReuse"] = reuse.into();
    }
    let helpers = layout.import_names();
    let mut module = WasmModule::new().map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut ids = HashMap::new();
    declare_user_functions_with_link_symbols_and_linkage(
        &mut module,
        &artifact,
        &mut ids,
        |name| match links.get(name) {
            Some(alias) if alias == entry => entry_link.as_ref().unwrap_or(alias).clone(),
            Some(alias) => alias.clone(),
            None => name.into(),
        },
        |name| if signatures.contains_key(name) || entry_link.as_deref() == Some(name) ||
            spawn_exports.iter().chain(&composition_exports).any(|entry| entry == name) { Linkage::Export } else { Linkage::Local },
    )?;
    beskid_codegen::contract_static::emit_witness_tables(&mut module, &artifact.witness_tables, &|name| ids.get(name).copied())?;
    let mut link_heap = needs_scope;
    for lowered in &artifact.functions {
        for (_, ext) in lowered.function.dfg.ext_funcs.iter() {
            if let ExternalName::TestCase(raw) = &ext.name {
                let name = std::str::from_utf8(raw.raw())?;
                let heap_import = name == beskid_abi::SYM_GC_COLLECT_IF_NEEDED
                    || (needs_scope && helpers.iter().any(|helper| helper == name))
                    || (string_pool.is_active() && string_pool.accepts_import(name));
                let clock_import = is_clock_import(name);
                let signature = &lowered.function.dfg.signatures[ext.signature];
                if clock_import {
                    ensure!(
                        artifact.corelib_service_symbols.contains(name),
                        "browser service {name} requires its compiler-authorized Corelib declaration"
                    );
                }
                if clock_import || name == beskid_abi::SYM_GC_COLLECT_IF_NEEDED {
                    ensure!(
                        signature.params.is_empty()
                            && signature.returns
                                == [cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I64)],
                        "browser service {name} requires () -> i64"
                    );
                }
                if !ids.contains_key(name)
                    && (heap_import
                        || clock_import
                        || (!artifact.browser_dynamic.is_empty()
                            && managed_clif::dynamic::admits_token_service(name, signature))
                        || fibers::admit(name, signature, &artifact.corelib_service_symbols, !artifact.spawn_artifacts.is_empty())?
                        || composition::service(name).map(|service| {
                            service.admit(signature, if name == composition::EXIT { composition_exit } else { composition_enabled })
                        }).transpose()?.unwrap_or(false)
                        || scheduler::find(name)
                            .map(|service| service.admit(signature, &artifact.corelib_service_symbols, true))
                            .transpose()?.unwrap_or(false)
                        || suspension::is_host_import(name, signature, &artifact.corelib_service_symbols)?
                        || stdio::admitted(name, signature, &artifact.corelib_service_symbols)?
                        || foundation::admitted(name, signature, &artifact.corelib_service_symbols)?
                        || network::service(name).is_some_and(|service| service.admits(signature, &artifact.corelib_service_symbols))
                        || protocol::admit(name, signature, &artifact.corelib_service_symbols)?
                        || text_services::is_host_import(name, signature, &artifact.corelib_service_symbols)?
                        || math::is_host_import(name, signature, &artifact.extern_imports)?)
                {
                    module.allow_import(name);
                    let id = module.declare_function(
                        name,
                        Linkage::Import,
                        &lowered.function.dfg.signatures[ext.signature],
                    )?;
                    ids.insert(name.to_owned(), id);
                    link_heap |= heap_import;
                }
                if !ids.contains_key(name) {
                    anyhow::bail!("{}", diagnostics::unavailable(name, &lowered.name, &artifact.browser_coverage, &artifact.extern_imports));
                }
            }
        }
    }
    let suspended: Vec<_> = [suspension::FIBER_YIELD, suspension::SLEEP_UNTIL, fibers::JOIN_STATUS]
        .into_iter().chain(scheduler::SERVICES.iter().filter(|service| service.suspends).map(|service| service.name))
        .chain(foundation::FILES).chain(stdio::READS)
        .chain(beskid_abi::runtime_source::browser_protocol::operations().iter()
            .filter(|operation| operation.suspends).map(|operation| operation.service.symbol))
        .chain(network::SERVICES.iter().filter(|service| service.suspends).map(|service| service.name))
        .filter(|name| needs_scope && ids.contains_key(*name))
        .collect();
    for lowered in artifact.functions {
        let mut cx = Context::for_function(lowered.function);
        remap_testcase_externals(&module, &mut cx, &ids)
            .with_context(|| format!("browser target, {}", lowered.name))?;
        // The bridge exposes an x64 ISA to satisfy Module, but its linear memory
        // uses wasm32 addresses. Verify symbolic addresses against its actual frontend config.
        for inst in cx.func.layout.blocks().flat_map(|block| cx.func.layout.block_insts(block)) {
            if let cranelift_codegen::ir::InstructionData::UnaryGlobalValue { .. } = cx.func.dfg.insts[inst] {
                ensure!(cx.func.dfg.value_type(cx.func.dfg.first_result(inst)) == module.target_config().pointer_type(),
                    "Wasm data address has the wrong pointer width in {}", lowered.name);
            }
        }
        verify_function(&cx.func, module.isa().flags()).map_err(|e| anyhow::anyhow!("{e}"))?;
        module.define_function(ids[&lowered.name], &mut cx)?;
    }
    let mut bytes = module.finish().map_err(|e| anyhow::anyhow!("{e}"))?;
    fibers::finalize_frames(&mut bytes, &fiber_frames)?;
    if let Some(link_name) = entry_link {
        bytes = rename_export(&bytes, &link_name, entry)?;
    }
    if link_heap || optimize {
        bytes = finish_with_binaryen(
            bytes,
            &signatures,
            link_heap,
            optimize,
            &layout,
            &string_pool,
            &internal_exports,
            &suspended,
            source_map,
        )?;
    }
    host_profile::append(&mut bytes, &signatures, needs_scope, &internal_exports, entry)?;
    if let Some(qualification) = qualification {
        coverage::append(&mut bytes, serde_json::to_vec(&qualification)?);
    }
    let map = source_map.then(|| source_map::attach(&mut bytes, &artifact.browser_coverage, &links)).transpose()?;
    output::publish(path, &bytes, map)
}

fn finish_with_binaryen(
    mut bytes: Vec<u8>,
    exports: &std::collections::BTreeMap<String, beskid_queries::SourceItemSignature>,
    link_heap: bool,
    optimize: bool,
    layout: &browser_heap::HeapLayout,
    string_pool: &strings::StringPool,
    internal_exports: &[&str],
    suspended: &[&str],
    debug_names: bool,
) -> Result<Vec<u8>> {
    let kit = binaryen::Binaryen::discover()?;
    if link_heap {
        let temp = tempfile::tempdir()?;
        let dir = temp.path();
        fs::write(dir.join("program.wasm"), bytes)?;
        fs::write(dir.join("heap.wasm"), browser_heap::encode_with_strings(layout, string_pool))?;
        let roots: Vec<_> = exports.keys().map(String::as_str)
            .chain(internal_exports.iter().copied())
            .map(|export| serde_json::json!({"name":export,"export":export,"root":true}))
            .collect();
        fs::write(dir.join("exports.json"), serde_json::to_vec(&roots)?)?;
        let companion = if suspended.is_empty() {
            "heap.wasm"
        } else {
            fs::write(dir.join("suspension.wasm"), suspension::encode_scoped_imports(suspended))?;
            run_binaryen(
                &kit,
                dir,
                debug_names,
                "wasm-merge",
                &["heap.wasm", "heap", "suspension.wasm", "suspension", "-o", "companion.wasm"],
            )?;
            "companion.wasm"
        };
        run_binaryen(&kit, dir, debug_names, "wasm-merge", &[companion, "env", "program.wasm", "program", "-o", "linked.wasm"])?;
        run_binaryen(&kit, dir, debug_names, "wasm-metadce", &["linked.wasm", "--graph-file", "exports.json", "-o", "final.wasm"])?;
        bytes = fs::read(dir.join("final.wasm"))?;
    }
    if optimize {
        let temp = tempfile::tempdir()?;
        fs::write(temp.path().join("input.wasm"), bytes)?;
        run_binaryen(&kit, temp.path(), debug_names, "wasm-opt", &["input.wasm", "-O2", "-o", "optimized.wasm"])?;
        bytes = fs::read(temp.path().join("optimized.wasm"))?;
    }
    Ok(bytes)
}

fn run_binaryen(kit: &binaryen::Binaryen, dir: &Path, debug_names: bool, tool: &str, args: &[&str]) -> Result<()> {
    let out = Command::new(kit.tool(tool))
        .args([
            "--mvp-features",
            "--enable-reference-types",
            "--enable-gc",
            "--enable-simd",
            "--enable-mutable-globals",
            "--enable-bulk-memory",
            "--enable-sign-ext",
            "--enable-multivalue",
            "--enable-nontrapping-float-to-int",
        ])
        .args(debug_names.then_some("-g"))
        .args(args)
        .current_dir(dir)
        .output()
        .with_context(|| format!("running browser compiler kit tool {tool}"))?;
    ensure!(out.status.success(), "{tool} failed: {}", String::from_utf8_lossy(&out.stderr));
    Ok(())
}
