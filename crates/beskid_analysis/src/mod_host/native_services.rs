//! Source-type callback mapping; native code never receives parent authority pointers.
use super::{
    native_correspondence::SyntaxCorrespondence,
    native_wire::{WireArena, WireTypes},
};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
fn node(v: &Value) -> Result<Value> {
    Ok(
        json!({"source_unit":v.get("sourceUnit").context("node source absent")?,"invocation_issuer":v.get("invocationIssuer").context("node issuer absent")?,"generation":v.get("syntaxGenerationId").context("node generation absent")?,"node":v.get("nodeId").context("node ID absent")?}),
    )
}
fn source_node(v: &Value) -> Result<Value> {
    Ok(
        json!({"sourceUnit":v.get("source_unit").context("node source absent")?,"invocationIssuer":v.get("invocation_issuer").context("node issuer absent")?,"syntaxGenerationId":v.get("generation").context("node generation absent")?,"nodeId":v.get("node").context("node ID absent")?}),
    )
}
fn bounds(v: &Value) -> Result<Value> {
    let nodes = v["maxNodes"].as_u64().context("negative/absent syntax node bound")?;
    let depth = v["maxDepth"].as_u64().context("negative/absent syntax depth bound")?;
    if nodes == 0 || nodes > 1000000 || depth == 0 || depth > 1024 {
        bail!("syntax bounds exceed invocation policy");
    }
    Ok(json!({"max_nodes":nodes,"max_depth":depth}))
}
fn some(v: Value) -> Value {
    json!({"Some":{"value":v}})
}

pub(crate) struct WorkerServices<'a> {
    pub plan: &'a Value,
    pub types: &'a WireTypes,
    pub syntax: &'a SyntaxCorrespondence,
    pub invocation: u64,
    pub sequence: u64,
    pub exchange: &'a mut dyn FnMut(&Value) -> Result<Value>,
}
impl WorkerServices<'_> {
    fn request(&mut self, operation: Value) -> Result<Value> {
        let sequence = self.sequence;
        self.sequence = self.sequence.checked_add(1).context("native callback sequence exhausted")?;
        let response = (self.exchange)(
            &json!({"version":2,"invocation":self.invocation,"sequence":sequence,"operation":operation}),
        )?;
        if response["version"] != 2 || response["sequence"] != sequence {
            bail!("native callback response is foreign/reordered");
        }
        Ok(response)
    }
    fn syntax(&mut self, request: Value) -> Result<Value> {
        let response = self.request(json!({"kind":"Syntax","request":request}))?;
        if let Some(error) = response.get("error") {
            bail!("native syntax callback rejected: {error}");
        }
        response.get("syntax").cloned().context("syntax response absent")
    }
    pub fn invoke(&mut self, operation: &str, arguments: &[u64], arena: &mut WireArena) -> Result<u64> {
        let callbacks = self.plan["callbacks"].as_array().context("native callback plan absent")?;
        let mut matches = callbacks.iter().filter(|c| c["operation"] == operation);
        let callback = matches.next().context("native callback is not source-issued")?.clone();
        if matches.next().is_some() {
            bail!("native callback identity ambiguous");
        }
        let parameters = callback["parameters"].as_array().context("callback parameters absent")?;
        if parameters.len() != arguments.len() {
            bail!("native callback argument count mismatch");
        }
        let args = parameters
            .iter()
            .zip(arguments)
            .map(|(ty, handle)| {
                self.types.json(arena, *handle, u32::try_from(ty.as_u64().context("callback type ID absent")?)?)
            })
            .collect::<Result<Vec<_>>>()?;
        let arg = |i: usize| args.get(i).context("native callback parameter absent");
        let result = if operation == "__mod_semantic_plan_canonical_paths" {
            let requested = arg(1)?.as_array().context("canonical path selector list absent")?;
            if requested.len() > 64
                || requested.iter().any(|value| value.as_str().is_none_or(|name| name.is_empty() || name.len() > 256))
            {
                bail!("canonical path selector bounds exceeded");
            }
            let response =
                self.request(json!({"kind":"PlanCanonicalPaths","reference":node(arg(0)?)?,"requested":requested}))?;
            if let Some(error) = response.get("error") {
                json!({"Error":{"error":{"code":"ModCanonicalPaths","message":error}}})
            } else {
                let paths =
                    response.get("paths").and_then(Value::as_array).context("canonical path response absent")?;
                if paths.len() != requested.len() {
                    bail!("canonical path response count differs");
                }
                let values=paths.iter().map(|path|Ok(json!({"logicalName":path.get("logical_name").context("canonical selector absent")?,"segments":path.get("segments").context("canonical route absent")?}))).collect::<Result<Vec<Value>>>()?;
                json!({"Ok":{"value":values}})
            }
        } else if operation == "__mod_semantic_resolve_function" {
            let path = arg(1)?.as_array().context("function path segment list absent")?;
            let path = path
                .iter()
                .map(|segment| segment.as_str().map(str::to_owned).context("function path segment is not text"))
                .collect::<Result<Vec<_>>>()?;
            if !super::callback_transport::function_path_in_bounds(&path) {
                bail!("function path bounds exceeded");
            }
            let response = self.request(json!({"kind":"ResolveFunction","reference":node(arg(0)?)?,"path":path}))?;
            super::native_semantic_values::function_result(&response)?
        } else if operation == "__mod_semantic_check_serializable" {
            let response = self.request(json!({"kind":"CheckSerializable","handle":arg(0)?["token"]}))?;
            super::native_semantic_values::serializable_result(&response)?
        } else if operation == "__mod_semantic_resolve_syntax_template" {
            let response = self.request(json!({"kind":"ResolveSyntaxTemplate","reference":node(arg(0)?)?}))?;
            if let Some(error) = response.get("error") {
                json!({"Error":{"error":{"code":"ModTemplate","message":error}}})
            } else {
                json!({"Ok":{"value":{"reference":source_node(response.get("template").context("template reference absent")?)?}}})
            }
        } else if operation == "__mod_semantic_resolve_syntax_type" {
            let response = self.request(json!({"kind":"ResolveSyntaxType","reference":node(arg(0)?)?}))?;
            super::native_semantic_values::semantic_result(&response, false)?
        } else if operation == "__mod_semantic_resolve_type" {
            let declaration = arg(0)?;
            let span = &declaration["span"];
            let response=self.request(json!({"kind":"ResolveType","source_unit":declaration["sourceUnit"],"generation":declaration["generation"],"node":declaration["node"],"span":{"start":span["start"],"end":span["end"],"line_start":span["lineStart"],"column_start":span["columnStart"],"line_end":span["lineEnd"],"column_end":span["columnEnd"]}}))?;
            super::native_semantic_values::semantic_result(&response, false)?
        } else if operation == "__mod_semantic_type_shape" {
            let response = self.request(json!({"kind":"TypeShape","handle":arg(0)?["token"]}))?;
            super::native_semantic_values::semantic_result(&response, true)?
        } else if operation == "__mod_semantic_capture_catchall" {
            let declaration = arg(1)?;
            let span = &declaration["span"];
            let response=self.request(json!({"kind":"CaptureCatchall","owner":arg(0)?["token"],"source_unit":declaration["sourceUnit"],"generation":declaration["generation"],"node":declaration["node"],
                "span":{"start":span["start"],"end":span["end"],"line_start":span["lineStart"],"column_start":span["columnStart"],"line_end":span["lineEnd"],"column_end":span["columnEnd"]}}))?;
            super::native_semantic_values::catchall_result(&response)?
        } else if operation == "__mod_semantic_validate_catchall" {
            let response = self.request(json!({"kind":"ValidateCatchall","token":arg(0)?}))?;
            super::native_semantic_values::catchall_result(&response)?
        } else {
            let query =
                operation.strip_prefix("__mod_query_").context("unknown source-issued native callback family")?;
            match query {
                "Children" | "Parent" | "Ancestors" | "Span" | "TrySpan" => {
                    let request = if query == "Ancestors" {
                        json!({"operation":"Ancestors","node":node(arg(0)?)?,"bounds":{"max_nodes":65536,"max_depth":128}})
                    } else {
                        json!({"operation":if query=="TrySpan"{"Span"}else{query},"node":node(arg(0)?)?})
                    };
                    let response = self.syntax(request)?;
                    match query {
                        "Children" | "Ancestors" => Value::Array(
                            response["Nodes"]
                                .as_array()
                                .context("syntax nodes absent")?
                                .iter()
                                .map(source_node)
                                .collect::<Result<Vec<_>>>()?,
                        ),
                        "Parent" => {
                            if response["Node"].is_null() {
                                json!("None")
                            } else {
                                some(source_node(&response["Node"])?)
                            }
                        }
                        "Span" | "TrySpan" => {
                            let span = &response["Span"];
                            if span.is_null() {
                                if query == "Span" {
                                    bail!("registered syntax node has no span");
                                }
                                json!("None")
                            } else {
                                let value = self.syntax.to_source("NodeSpan", span)?;
                                if query == "TrySpan" { some(value) } else { value }
                            }
                        }
                        _ => unreachable!(),
                    }
                }
                "Descendants" | "Select" | "OfKind" | "FindFirst" => {
                    let q = arg(0)?;
                    let mut request = json!({"operation":if query=="Select"{"Descendants"}else{query},"node":node(&q["start"])?,"bounds":bounds(&q["bounds"])?});
                    if matches!(query, "OfKind" | "FindFirst") {
                        request["kind"] = arg(1)?.clone();
                    }
                    let response = self.syntax(request)?;
                    if query == "FindFirst" {
                        if response["Node"].is_null() { json!("None") } else { some(source_node(&response["Node"])?) }
                    } else {
                        let nodes = Value::Array(
                            response["Nodes"]
                                .as_array()
                                .context("syntax descendants absent")?
                                .iter()
                                .map(source_node)
                                .collect::<Result<Vec<_>>>()?,
                        );
                        if query == "Select" { json!({"nodes":nodes,"bounds":q["bounds"]}) } else { nodes }
                    }
                }
                "AtProgram" => {
                    let root = self.syntax(json!({"operation":"Root"}))?;
                    let root = source_node(&root["Node"])?;
                    let projection =
                        self.syntax(json!({"operation":"Project","node":node(&root)?,"kind":"Program"}))?;
                    let actual = self.syntax.to_source("Program", &projection["Projection"]["value"])?;
                    if &actual != arg(0)? {
                        bail!("AtProgram argument is not the current registered program");
                    }
                    json!({"start":root,"bounds":{"maxNodes":65536,"maxDepth":128}})
                }
                "WhereKind" => {
                    let selection = arg(0)?;
                    let policy = bounds(&selection["bounds"])?;
                    let nodes = selection["nodes"].as_array().context("selection nodes absent")?;
                    let limit = policy["max_nodes"].as_u64().context("selection limit absent")? as usize;
                    if nodes.len() > limit {
                        bail!("selection exceeds explicit node budget");
                    }
                    let mut kept = Vec::new();
                    for selected in nodes {
                        let response =
                            self.syntax(json!({"operation":"Project","node":node(selected)?,"kind":arg(1)?}))?;
                        if !response["Projection"]["value"].is_null() {
                            kept.push(selected.clone());
                        }
                    }
                    json!({"nodes":kept,"bounds":selection["bounds"]})
                }
                "At" => {
                    self.syntax(json!({"operation":"Children","node":node(arg(0)?)?}))?;
                    json!({"start":arg(0)?,"bounds":{"maxNodes":65536,"maxDepth":128}})
                }
                "Pipeline" => {
                    bounds(arg(1)?)?;
                    self.syntax(json!({"operation":"Children","node":node(arg(0)?)?}))?;
                    json!({"root":arg(0)?,"bounds":arg(1)?})
                }
                "Replace" | "Remove" | "InsertBefore" | "InsertAfter" | "Apply" => {
                    let pipeline = arg(0)?;
                    let mut request = json!({"operation":query,"root":node(&pipeline["root"])? ,"bounds":bounds(&pipeline["bounds"])?});
                    match query {
                        "Replace" => {
                            request["target"] = node(arg(1)?)?;
                            request["replacement"] = node(arg(2)?)?;
                        }
                        "Remove" => request["target"] = node(arg(1)?)?,
                        "InsertBefore" | "InsertAfter" => {
                            request["anchor"] = node(arg(1)?)?;
                            request["node"] = node(arg(2)?)?;
                        }
                        _ => {}
                    }
                    let response = self.syntax(request)?;
                    if query == "Apply" { source_node(&response["Node"])? } else { pipeline.clone() }
                }
                _ if query.starts_with("As") => {
                    let kind = &query[2..];
                    let response = self.syntax(json!({"operation":"Project","node":node(arg(0)?)?,"kind":kind}))?;
                    let projection = &response["Projection"]["value"];
                    if projection.is_null() { json!("None") } else { some(self.syntax.to_source(kind, projection)?) }
                }
                _ => bail!("source query callback {query} lacks executable mapping"),
            }
        };
        let ty = u32::try_from(callback["result_type"].as_u64().context("callback result type absent")?)?;
        self.types.build(arena, ty, &result)
    }
}
