//! Isolated worker orchestration; producer witnesses remain owned by AOT.
use super::{
    ModSyntaxBounds, ModSyntaxNodeRef, ModSyntaxRequest, ModSyntaxResponse, callback_transport::NativeSemanticSession,
    native_channel::ParentChannel, native_correspondence::SyntaxCorrespondence,
};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::{io::Write, path::Path, process::Command};
pub struct NativeInvocationOutput {
    pub value: Value,
    pub compiled_metadata: Vec<super::ModCompiledMetadata>,
}
fn checked_context(session: &NativeSemanticSession<'_>, mut context: Value) -> Result<Value> {
    let root = session.source_root_value()?;
    let compilation =
        context.get_mut("compilation").and_then(Value::as_object_mut).context("native compilation context absent")?;
    compilation.insert("entryRoot".into(), root);
    compilation.insert("syntaxGenerationId".into(), json!(session.generation()));
    Ok(context)
}
fn run_worker<'a>(
    descriptor: &super::ModArtifactDescriptor,
    runtime_prefix: &Path,
    worker: &Path,
    symbol: &str,
    session: NativeSemanticSession<'a>,
    request: Value,
    factory: Value,
    control: &beskid_execution::NativeExecutionControl,
) -> Result<(NativeInvocationOutput, NativeSemanticSession<'a>)> {
    descriptor.validate(runtime_prefix)?;
    let initial = json!({"descriptor":descriptor.sidecar_path(),"descriptor_sha256":super::descriptor::native_mod_file_sha256(&descriptor.sidecar_path())?,"runtime_prefix":runtime_prefix,"symbol":symbol,"generation":session.generation(),"invocation":session.invocation(),"request":request,"factory_request":factory});
    let directory = tempfile::Builder::new().prefix("beskid-native-mod-").rand_bytes(32).tempdir()?;
    let token = directory
        .path()
        .file_name()
        .and_then(|name| name.to_str())
        .context("private worker token is not UTF8")?
        .to_owned();
    let mut channel = ParentChannel::new(session, token.clone(), initial)?;
    let endpoint = directory.path().join("endpoint.json");
    let mut file = std::fs::OpenOptions::new().create_new(true).write(true).open(&endpoint)?;
    file.write_all(&serde_json::to_vec(&json!({"address":channel.address()?,"token":token}))?)?;
    file.sync_all()?;
    drop(file);
    let mut command = Command::new(worker.canonicalize()?);
    command.args(["dev", "native-mod-worker", "--endpoint"]).arg(&endpoint);
    let output = control.run_command_serviced(&mut command, directory.path(), "native Mod worker", || {
        channel.poll().map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
    })?;
    if !output.status.success() {
        bail!("native Mod worker failed ({}) : {}", output.status, String::from_utf8_lossy(&output.stderr))
    }
    channel.poll()?;
    let result = channel.completion()?;
    Ok((result, channel.into_session()))
}
#[doc(hidden)]
pub fn invoke_qualified_native_transport(
    descriptor: &super::ModArtifactDescriptor,
    runtime_prefix: &Path,
    worker: &Path,
    symbol: &str,
    context: Value,
    targets: &[String],
    family: &str,
    authority: &dyn super::ModSemanticAuthority,
    control: &beskid_execution::NativeExecutionControl,
) -> Result<NativeInvocationOutput> {
    descriptor.validate(runtime_prefix)?;
    if !descriptor
        .registrations
        .iter()
        .any(|r| r.entry_symbol == symbol && r.contract_id == format!("Beskid.Compiler.Collect.{family}"))
    {
        bail!("native invocation not in qualified registration closure")
    }
    let session = NativeSemanticSession::new(authority)?;
    let context = checked_context(&session, context)?;
    let request = match family {
        "Collector" => context.clone(),
        "Generator" | "GrammarGenerator" => json!({"context":context,"targets":{"targetIds":targets}}),
        "Analyzer" | "AttributeGenerator" => json!({"context":context}),
        _ => bail!("native family requires specialized typed dispatch"),
    };
    let (mut output, session) =
        run_worker(descriptor, runtime_prefix, worker, symbol, session, request, context, control)?;
    if matches!(family, "Generator" | "GrammarGenerator") {
        let schema = std::fs::read(descriptor.artifact_dir.join("sdk/schema.json"))?;
        let decoded = super::native_results::native_generator_result("", &output.value, &schema)?;
        let bindings =
            output.value.get("targetBindings").and_then(Value::as_array).context("native target bindings absent")?;
        if bindings.len() > 65536 {
            bail!("native target binding count exceeded");
        }
        let mut indices = std::collections::HashSet::new();
        for binding in bindings {
            let index = binding
                .get("contributionIndex")
                .and_then(Value::as_u64)
                .and_then(|v| u32::try_from(v).ok())
                .context("target binding index invalid")?;
            if !indices.insert(index) {
                bail!("duplicate target binding index");
            }
            let item = decoded.typed_items.get(index as usize).context("target binding index outside contributions")?;
            let token = binding
                .get("target")
                .and_then(|v| v.get("token"))
                .and_then(Value::as_u64)
                .context("target binding handle invalid")?;
            output.compiled_metadata.push(session.compile_target(index, token, item)?);
        }
        let templates = output
            .value
            .get("templateBindings")
            .and_then(Value::as_array)
            .context("native template bindings absent")?;
        if bindings.len().checked_add(templates.len()).is_none_or(|count| count > 65536) {
            bail!("native template/target binding count exceeded");
        }
        for binding in templates {
            let index = binding
                .get("contributionIndex")
                .and_then(Value::as_u64)
                .and_then(|v| u32::try_from(v).ok())
                .context("template binding index invalid")?;
            if !indices.insert(index) {
                bail!("duplicate template/target binding index");
            }
            let item =
                decoded.typed_items.get(index as usize).context("template binding index outside contributions")?;
            let source = binding
                .get("template")
                .and_then(|v| v.get("reference"))
                .context("template binding issued reference absent")?;
            let reference: super::ModSyntaxNodeRef = serde_json::from_value(json!({
                "source_unit":source.get("sourceUnit").context("template source absent")?,
                "invocation_issuer":source.get("invocationIssuer").context("template issuer absent")?,
                "generation":source.get("syntaxGenerationId").context("template generation absent")?,
                "node":source.get("nodeId").context("template node absent")?
            }))?;
            output.compiled_metadata.push(session.compile_template(index, &reference, item)?);
        }
    }
    Ok(output)
}
fn node_type(plan: &Value, type_id: u64) -> Result<&str> {
    plan["syntax_types"]
        .as_array()
        .context("native canonical syntax proofs absent")?
        .iter()
        .find(|t| t["type_id"].as_u64() == Some(type_id))
        .and_then(|t| t["schema_type"].as_str())
        .context("native type lacks canonical SDK syntax proof")
}
fn syntax_type(name: &str) -> Result<(&str, crate::syntax_query::NodeKind, bool)> {
    let name = name.strip_prefix("Beskid.Syntax.Nodes.").context("Rewriter type is not canonical SDK syntax")?;
    let (kind, spanned) = name.strip_prefix("Spanned").map_or((name, false), |kind| (kind, true));
    Ok((name, serde_json::from_value(json!(kind)).context("Rewriter type is not an AST node kind")?, spanned))
}
fn project_source(
    session: &NativeSemanticSession<'_>,
    correspondence: &SyntaxCorrespondence,
    node: &ModSyntaxNodeRef,
    name: &str,
    kind: crate::syntax_query::NodeKind,
    spanned: bool,
) -> Result<Value> {
    let ModSyntaxResponse::Projection { value: Some(mut value), .. } =
        session.syntax_query(&ModSyntaxRequest::Project { node: node.clone(), kind })?
    else {
        bail!("registered Rewriter node projection absent")
    };
    if spanned {
        let ModSyntaxResponse::Span(Some(span)) =
            session.syntax_query(&ModSyntaxRequest::Span { node: node.clone() })?
        else {
            bail!("registered Rewriter span absent")
        };
        value = json!({"node":value,"span":span,"id":node.node});
    }
    correspondence.to_source(name, &value)
}
#[doc(hidden)]
pub fn invoke_qualified_native_rewriter(
    descriptor: &super::ModArtifactDescriptor,
    runtime_prefix: &Path,
    worker: &Path,
    symbol: &str,
    context: Value,
    authority: &dyn super::ModSemanticAuthority,
    control: &beskid_execution::NativeExecutionControl,
) -> Result<super::RewriterOutcome> {
    descriptor.validate(runtime_prefix)?;
    let registration = descriptor
        .registrations
        .iter()
        .find(|r| r.entry_symbol == symbol && r.contract_id == "Beskid.Compiler.Collect.Rewriter")
        .context("native Rewriter registration absent")?;
    let plan: Value = serde_json::from_slice(&std::fs::read(descriptor.artifact_dir.join("adapter-plan.json"))?)?;
    let entries = plan["entries"]
        .as_array()
        .context("native entries absent")?
        .iter()
        .filter(|e| e["symbol"] == symbol)
        .collect::<Vec<_>>();
    if entries.len() != 1 {
        bail!("native Rewriter entry ambiguous")
    };
    let entry = entries[0];
    let source_name = node_type(&plan, entry["request_type"].as_u64().context("native request type absent")?)?;
    let (source_name, source_kind, source_spanned) = syntax_type(source_name)?;
    let result = plan["types"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == entry["result_type"])
        .context("Rewriter result type absent")?;
    let variants = result["kind"]["Enum"]["variants"].as_array().context("Rewriter result is not exact Result enum")?;
    let ok = variants.iter().find(|v| v["name"] == "Ok").context("Rewriter success variant absent")?;
    let fields = ok["fields"].as_array().context("Rewriter success fields absent")?;
    if fields.len() != 1 || fields[0]["name"] != "value" {
        bail!("Rewriter success payload mismatch")
    }
    let (target_name, target_kind, target_spanned) =
        syntax_type(node_type(&plan, fields[0]["type_id"].as_u64().context("Rewriter target type absent")?)?)?;
    let correspondence = SyntaxCorrespondence::read(&std::fs::read(descriptor.artifact_dir.join("sdk/schema.json"))?)?;
    let mut session = NativeSemanticSession::new(authority)?;
    let ModSyntaxResponse::Node(Some(root)) = session.syntax_query(&ModSyntaxRequest::Root)? else {
        bail!("Rewriter root absent")
    };
    let bounds = ModSyntaxBounds { max_nodes: 65536, max_depth: 128 };
    let ModSyntaxResponse::Nodes(nodes) =
        session.syntax_query(&ModSyntaxRequest::OfKind { node: root.clone(), bounds, kind: source_kind })?
    else {
        bail!("Rewriter target selection absent")
    };
    let mut applied = 0u32;
    for node in nodes {
        control.check("native Rewriter target")?;
        let request = project_source(&session, &correspondence, &node, source_name, source_kind, source_spanned)?;
        let factory = checked_context(&session, context.clone())?;
        let (output, returned) =
            run_worker(descriptor, runtime_prefix, worker, symbol, session, request, factory, control)?;
        session = returned;
        let object = output.value.as_object().context("Rewriter Result requires enum")?;
        if object.len() != 1 {
            bail!("Rewriter Result discriminants invalid")
        }
        if let Some(ok) = object.get("Ok") {
            let mut projection =
                correspondence.to_rust(target_name, ok.get("value").context("Rewriter target payload absent")?)?;
            if target_spanned {
                let wrapper = projection.as_object().context("Rewriter target wrapper requires record")?;
                if wrapper.len() != 3
                    || wrapper.get("id").and_then(Value::as_u64).filter(|id| *id <= u32::MAX as u64).is_none()
                {
                    bail!("Rewriter target wrapper identity invalid");
                }
                let span =
                    wrapper.get("span").and_then(Value::as_object).context("Rewriter target wrapper span absent")?;
                let start = span.get("start").and_then(Value::as_u64).context("Rewriter target span start invalid")?;
                let end = span.get("end").and_then(Value::as_u64).context("Rewriter target span end invalid")?;
                if start > end {
                    bail!("Rewriter target span bounds reversed");
                }
                // Replacement authority retains the registered target's outer provenance.
                projection = wrapper.get("node").context("Rewriter target wrapper node absent")?.clone()
            };
            session.syntax_query(&ModSyntaxRequest::ReplaceProjection {
                root: root.clone(),
                target: node,
                kind: target_kind,
                value: projection,
                bounds,
            })?;
            applied = applied.checked_add(1).context("Rewriter count overflow")?;
        } else if !object.contains_key("Error") {
            bail!("Rewriter Result foreign variant")
        }
    }
    if applied > 0 {
        session.syntax_query(&ModSyntaxRequest::Apply { root, bounds })?;
    }
    Ok(super::RewriterOutcome { type_id: registration.type_id.clone(), applied_fix_count: applied, edits: Vec::new() })
}
