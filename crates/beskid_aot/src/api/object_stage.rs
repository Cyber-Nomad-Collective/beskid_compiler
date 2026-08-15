use std::collections::HashSet;
use std::path::PathBuf;

use beskid_pipeline::{observe_phase_result, phases::AOT_EMIT_OBJECT};

use crate::error::AotResult;
use crate::export_table::ExportTable;
use crate::object_module::BeskidObjectModule;
use crate::target::detect_target;

use super::model::AotBuildRequest;
use super::platform_objects::compile_core_args_entry_adapter;
use super::validation::{apply_export_policy, core_args_entry_adapter};

#[derive(Debug, Clone)]
pub(super) struct ObjectStageResult {
    pub(super) object_path: PathBuf,
    pub(super) exported_symbols: Vec<String>,
    pub(super) additional_object_paths: Vec<PathBuf>,
    pub(super) executable_entry: Option<String>,
}
pub(super) fn emit_object_stage(req: &AotBuildRequest) -> AotResult<ObjectStageResult> {
    let target = detect_target(req.target_triple.as_deref())?;
    let object_path = req.object_path.clone().unwrap_or_else(|| req.output_path.with_extension(target.object_ext));

    let entry_adapter = core_args_entry_adapter(&req.artifact, &target.triple)?;
    let exports = req.artifact.exports.clone();
    let all_symbols = req
        .artifact
        .functions
        .iter()
        .map(|function| {
            if let Some(adapter) =
                entry_adapter.filter(|_| function.name.split('#').next().is_some_and(|name| name == "Main"))
            {
                adapter.program_entry.to_owned()
            } else {
                beskid_codegen::object_link_symbol(&function.name, &exports)
            }
        })
        .collect::<Vec<_>>();
    let export_table = ExportTable::from_artifact(&req.artifact);
    let export_policy = export_table.resolve_export_policy(&req.export_policy);
    let exported_symbols = apply_export_policy(all_symbols, &export_policy);
    let exported_symbol_set = exported_symbols.iter().cloned().collect::<HashSet<_>>();

    let mut object_module = BeskidObjectModule::new(req.target_triple.as_deref(), req.profile)?;
    let obs = req.pipeline.as_deref();
    observe_phase_result(obs, AOT_EMIT_OBJECT, || {
        object_module.compile_artifact_with_exports_and_entry_adapter(
            &req.artifact,
            &exported_symbol_set,
            entry_adapter.map(|adapter| adapter.program_entry),
            obs,
        )
    })?;

    object_module.finalize_to_path(&object_path)?;

    let additional_object_paths = if let Some(adapter) = entry_adapter {
        vec![compile_core_args_entry_adapter(
            adapter,
            object_path.parent().unwrap_or_else(|| std::path::Path::new(".")),
            "beskid",
        )?]
    } else {
        Vec::new()
    };
    Ok(ObjectStageResult {
        object_path,
        exported_symbols,
        additional_object_paths,
        executable_entry: entry_adapter.map(|adapter| adapter.executable_entry.to_owned()),
    })
}
