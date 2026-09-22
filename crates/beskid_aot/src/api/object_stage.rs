use std::collections::HashSet;
use std::path::PathBuf;

use beskid_pipeline::{observe_phase_result, phases::AOT_EMIT_OBJECT};

use crate::error::AotResult;
use crate::export_table::ExportTable;
use crate::object_module::{
    BeskidObjectModule, EXECUTABLE_PROGRAM_ENTRY, ExecutableEntrySymbol, emitted_object_symbol,
};
use crate::target::detect_target;

use super::model::{AotBuildRequest, BuildOutputKind, ExportPolicy};
use super::platform_objects::compile_executable_bootstrap;
use super::validation::{apply_export_policy, core_args_entry_adapter};

#[derive(Debug, Clone)]
pub(super) struct ObjectStageResult {
    pub(super) object_path: PathBuf,
    pub(super) exported_symbols: Vec<String>,
    pub(super) additional_object_paths: Vec<PathBuf>,
    pub(super) executable_program_entry: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use beskid_codegen::{CodegenArtifact, LoweredFunction};
    use cranelift_codegen::{
        cursor::{Cursor, FuncCursor},
        ir::InstBuilder,
    };
    use object::{Object, ObjectSymbol};

    fn custom_entry_request() -> (tempfile::TempDir, AotBuildRequest) {
        let mut function = cranelift_codegen::ir::Function::new();
        let block = function.dfg.make_block();
        function.layout.append_block(block);
        FuncCursor::new(&mut function).at_bottom(block).ins().return_(&[]);
        let artifact = CodegenArtifact {
            functions: vec![LoweredFunction { name: "Start#0".into(), function }],
            ..Default::default()
        };
        let temp = tempfile::tempdir().unwrap();
        let mut request =
            AotBuildRequest::with_defaults(artifact, BuildOutputKind::ObjectOnly, temp.path().join("app"), "Start");
        request.output_kind = BuildOutputKind::Exe;
        (temp, request)
    }

    #[test]
    fn unannotated_custom_entry_is_linkable_without_a_public_host_export() {
        let (_temp, request) = custom_entry_request();
        let result = emit_object_stage(&request).expect("emit custom unit entry and its host");
        assert!(result.exported_symbols.is_empty(), "host entry leaked: {:?}", result.exported_symbols);
        let bytes = std::fs::read(&result.object_path).unwrap();
        let object = object::File::parse(bytes.as_slice()).unwrap();
        assert!(
            object.symbols().any(|symbol| {
                symbol.name().is_ok_and(|name| name.trim_start_matches('_') == "beskid_program_main")
                    && symbol.is_global()
                    && symbol.is_definition()
            }),
            "the host still needs a defined external linkage symbol"
        );
    }

    #[test]
    fn executable_rejects_explicit_exports_that_collide_with_its_startup() {
        let (_temp, mut request) = custom_entry_request();
        request.artifact.exports.push(beskid_codegen::ExportEntry {
            beskid_name: "Start".into(),
            exported_symbol: "main".into(),
            abi: "C".into(),
        });
        let error = emit_object_stage(&request).expect_err("main belongs to the bootstrap");
        assert!(
            matches!(error, crate::error::AotError::InvalidRequest { message } if message.contains("main") && message.contains("bootstrap"))
        );
    }

    #[test]
    fn explicitly_requested_host_boundary_remains_a_public_export() {
        let (_temp, mut request) = custom_entry_request();
        request.export_policy = ExportPolicy::Explicit(vec!["beskid_program_main".into()]);
        let result = emit_object_stage(&request).unwrap();
        assert_eq!(result.exported_symbols, ["beskid_program_main"]);
    }

    #[test]
    fn executable_rejects_a_selected_entry_export_alias_it_cannot_emit() {
        let (_temp, mut request) = custom_entry_request();
        request.artifact.exports.push(beskid_codegen::ExportEntry {
            beskid_name: "Start".into(),
            exported_symbol: "PublicStart".into(),
            abi: "C".into(),
        });
        let error = emit_object_stage(&request).expect_err("a selected-entry alias cannot be silently discarded");
        assert!(
            matches!(error, crate::error::AotError::InvalidRequest { message } if message.contains("PublicStart") && message.contains("beskid_program_main"))
        );
    }

    #[test]
    fn executable_preserves_an_explicitly_exported_unselected_main_helper() {
        let (_temp, mut request) = custom_entry_request();
        request.artifact.extern_imports.push(beskid_codegen::ExternImport {
            symbol: "beskid_rt_v5_args_count".into(),
            abi: Some("C".into()),
            library: None,
        });
        let mut helper = request.artifact.functions[0].clone();
        helper.name = "Main#1".into();
        request.artifact.functions.push(helper);
        request.artifact.exports.push(beskid_codegen::ExportEntry {
            beskid_name: "Main".into(),
            exported_symbol: "PublicHelper".into(),
            abi: "C".into(),
        });
        let result = emit_object_stage(&request).unwrap();
        assert_eq!(result.exported_symbols, ["PublicHelper"]);
        request.artifact.exports[0].exported_symbol = "beskid_program_main".into();
        assert!(matches!(emit_object_stage(&request), Err(crate::error::AotError::InvalidRequest { .. })));
    }

    #[test]
    fn core_args_executable_reserves_both_crt_startup_names_before_emission() {
        for target in ["x86_64-pc-windows-msvc", "aarch64-apple-darwin", "x86_64-unknown-linux-gnu"] {
            for startup in ["main", "wmain"] {
                let (_temp, mut request) = custom_entry_request();
                request.target_triple = Some(target.into());
                request.artifact.extern_imports.push(beskid_codegen::ExternImport {
                    symbol: "beskid_rt_v5_args_count".into(),
                    abi: Some("C".into()),
                    library: None,
                });
                let mut helper = request.artifact.functions[0].clone();
                helper.name = "Helper#1".into();
                request.artifact.functions.push(helper);
                request.artifact.exports.push(beskid_codegen::ExportEntry {
                    beskid_name: "Helper".into(),
                    exported_symbol: startup.into(),
                    abi: "C".into(),
                });
                let error = emit_object_stage(&request).expect_err("both CRT startup names are reserved");
                assert!(
                    matches!(&error, crate::error::AotError::InvalidRequest { message } if message.contains(startup) && message.contains("bootstrap")),
                    "{target} must reject helper export {startup} before target compilation: {error}",
                );
            }
        }
    }
}
pub(super) fn emit_object_stage(req: &AotBuildRequest) -> AotResult<ObjectStageResult> {
    let target = detect_target(req.target_triple.as_deref())?;
    let object_path = req.object_path.clone().unwrap_or_else(|| req.output_path.with_extension(target.object_ext));

    let entry_adapter = core_args_entry_adapter(&req.artifact, &target.triple)?;
    let exports = req.artifact.exports.clone();
    let executable_entry = if req.output_kind == BuildOutputKind::Exe {
        if req
            .artifact
            .functions
            .iter()
            .any(|function| function.name.split('#').next() == Some(req.entrypoint.as_str()))
        {
            Some(ExecutableEntrySymbol { logical: &req.entrypoint, symbol: EXECUTABLE_PROGRAM_ENTRY })
        } else {
            return Err(crate::error::AotError::MissingEntrypoint { symbol: req.entrypoint.clone() });
        }
    } else {
        None
    };
    if let Some(entry) = executable_entry {
        if let Some(export) = exports.iter().find(|export| {
            matches!(export.exported_symbol.as_str(), "main" | "wmain")
                || (export.exported_symbol == entry.symbol
                    && export.beskid_name.split('#').next() != Some(entry.logical))
        }) {
            return Err(crate::error::AotError::InvalidRequest {
                message: format!("explicit export `{}` collides with the executable bootstrap", export.exported_symbol),
            });
        }
        if let Some(export) = exports.iter().find(|export| {
            export.beskid_name.split('#').next() == Some(entry.logical) && export.exported_symbol != entry.symbol
        }) {
            return Err(crate::error::AotError::InvalidRequest {
                message: format!(
                    "selected executable entry `{}` cannot export alias `{}`; its host boundary is `{}`",
                    entry.logical, export.exported_symbol, entry.symbol,
                ),
            });
        }
    }
    let all_symbols = req
        .artifact
        .functions
        .iter()
        .map(|function| emitted_object_symbol(&function.name, &exports, executable_entry))
        .collect::<Vec<_>>();
    let export_table = ExportTable::from_artifact(&req.artifact);
    let export_policy = export_table.resolve_export_policy(&req.export_policy);
    let mut exported_symbols = apply_export_policy(all_symbols, &export_policy);
    if let Some(entry) = executable_entry {
        // The bootstrap needs external object linkage, which is independent of
        // the artifact's public API. Only an explicit symbol request exports it.
        if !matches!(&export_policy, ExportPolicy::Explicit(symbols) if symbols.iter().any(|symbol| symbol == entry.symbol))
        {
            exported_symbols.retain(|symbol| symbol != entry.symbol);
        }
    }
    let mut linkage_symbol_set = exported_symbols.iter().cloned().collect::<HashSet<_>>();
    if let Some(entry) = executable_entry {
        linkage_symbol_set.insert(entry.symbol.to_owned());
    }

    let mut object_module = BeskidObjectModule::new(req.target_triple.as_deref(), req.profile)?;
    let obs = req.pipeline.as_deref();
    observe_phase_result(obs, AOT_EMIT_OBJECT, || {
        object_module.compile_artifact_with_exports_and_executable_entry(
            &req.artifact,
            &linkage_symbol_set,
            executable_entry,
            obs,
        )
    })?;

    object_module.finalize_to_path(&object_path)?;

    let additional_object_paths = if executable_entry.is_some() {
        let returns_void = req
            .artifact
            .functions
            .iter()
            .find(|function| function.name.split('#').next() == Some(req.entrypoint.as_str()))
            .is_some_and(|function| function.function.signature.returns.is_empty());
        vec![compile_executable_bootstrap(
            &target.triple,
            entry_adapter,
            object_path.parent().unwrap_or_else(|| std::path::Path::new(".")),
            "beskid",
            returns_void,
        )?]
    } else {
        Vec::new()
    };
    Ok(ObjectStageResult {
        object_path,
        exported_symbols,
        additional_object_paths,
        executable_program_entry: executable_entry.map(|entry| entry.symbol.to_owned()),
    })
}
