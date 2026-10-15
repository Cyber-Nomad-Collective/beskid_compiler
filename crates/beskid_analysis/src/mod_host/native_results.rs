//! Typed result extraction after the private producer invokes its exact CABI2 entry.
use super::{invoker::*, native_correspondence::SyntaxCorrespondence};
use anyhow::{Context, Result, bail};
use serde_json::Value;
fn string(value: &Value, key: &str) -> Result<String> {
    Ok(value.get(key).and_then(Value::as_str).context(format!("native result missing string {key}"))?.into())
}
fn array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>> {
    let values = value.get(key).and_then(Value::as_array).context(format!("native result missing array {key}"))?;
    if values.len() > 65536 {
        bail!("native result count budget exceeded")
    }
    Ok(values)
}
#[doc(hidden)]
pub fn native_collector_result(type_id: &str, value: &Value) -> Result<CollectorOutcome> {
    Ok(CollectorOutcome {
        type_id: type_id.into(),
        narrowed_targets: array(value, "targetIds")?
            .iter()
            .map(|v| v.as_str().map(str::to_owned).context("native target must be string"))
            .collect::<Result<_>>()?,
    })
}
/// A native result object must carry exactly the fields of its SDK result type. A field
/// outside that set (for example a retired source-text contribution) rejects the result.
fn exact_fields(value: &Value, fields: &[&str]) -> Result<()> {
    let object = value.as_object().context("native result must be an SDK record")?;
    if let Some(foreign) = object.keys().find(|key| !fields.contains(&key.as_str())) {
        bail!("native result carries a field outside its typed SDK contract: {foreign}")
    }
    if object.len() != fields.len() {
        bail!("native result is missing a field of its typed SDK contract")
    }
    Ok(())
}
/// Decode a Generator result. Only typed syntax contributions and their issuer bindings are
/// admitted; the host has no source-text contribution path.
#[doc(hidden)]
pub fn native_generator_result(type_id: &str, value: &Value, schema: &[u8]) -> Result<GeneratorOutcome> {
    exact_fields(value, &["items", "targetBindings", "templateBindings"])?;
    let correspondence = SyntaxCorrespondence::read(schema)?;
    let mut outcome = GeneratorOutcome { compiled_metadata: Vec::new(), type_id: type_id.into(), ..Default::default() };
    for item in array(value, "items")? {
        let object = item.as_object().context("native syntax contribution must be enum")?;
        if object.len() != 1 {
            bail!("native syntax contribution discriminant count")
        }
        let (kind, payload) = object.iter().next().unwrap();
        let schema_type = match kind.as_str() {
            "ContractDefinition" => "SpannedContractDefinition",
            "TypeDefinition" => "SpannedTypeDefinition",
            "FunctionDefinition" => "SpannedFunctionDefinition",
            "ImplBlock" => "SpannedImplBlock",
            _ => bail!("native contribution kind not admitted"),
        };
        let projected = correspondence
            .to_rust(schema_type, payload.get("definition").context("native contribution declaration absent")?)?;
        let variant = if kind == "FunctionDefinition" { "Function" } else { kind.as_str() };
        let mut node = serde_json::Map::new();
        node.insert(variant.into(), projected.clone());
        let item = serde_json::json!({"node":node,"span":projected["span"],"id":projected["id"]});
        outcome.typed_items.push(serde_json::from_value(item)?);
    }
    Ok(outcome)
}
fn edits(value: &Value) -> Result<Vec<RewriteEdit>> {
    array(value, "edits")?
        .iter()
        .map(|edit| {
            let start = usize::try_from(edit["start"].as_u64().context("edit start absent")?)?;
            let end = usize::try_from(edit["end"].as_u64().context("edit end absent")?)?;
            if start > end {
                bail!("native edit bounds reversed")
            };
            Ok(match edit["kind"].as_u64().context("edit kind absent")? {
                0 if start == end => RewriteEdit::Insert { offset: start, text: string(edit, "text")? },
                1 => RewriteEdit::Replace { start, end, text: string(edit, "text")? },
                2 if string(edit, "text")?.is_empty() => RewriteEdit::Delete { start, end },
                _ => bail!("native edit kind or payload invalid"),
            })
        })
        .collect()
}
#[doc(hidden)]
pub fn native_analyzer_result(type_id: &str, value: &Value, source_len: usize) -> Result<AnalyzerOutcome> {
    let mut outcome = AnalyzerOutcome { type_id: type_id.into(), ..Default::default() };
    for item in array(value, "diagnostics")? {
        let start = usize::try_from(item["spanStart"].as_u64().context("diagnostic start absent")?)?;
        let end = usize::try_from(item["spanEnd"].as_u64().context("diagnostic end absent")?)?;
        if start > end || end > source_len {
            bail!("native diagnostic outside source")
        };
        let severity = match item["severity"].as_str().context("native severity absent")? {
            "Error" => AnalyzerSeverity::Error,
            "Warning" => AnalyzerSeverity::Warning,
            "Note" => AnalyzerSeverity::Note,
            _ => bail!("native diagnostic severity invalid"),
        };
        outcome.diagnostics.push(AnalyzerDiagnostic {
            code: string(item, "code")?,
            message: string(item, "message")?,
            severity,
            span: if start == 0 && end == 0 { None } else { Some((start, end)) },
        })
    }
    for item in array(value, "fixes")? {
        let diagnostic_index = u32::try_from(item["diagnosticIndex"].as_u64().context("native fix index absent")?)?;
        if diagnostic_index as usize >= outcome.diagnostics.len() {
            bail!("native fix targets absent diagnostic")
        };
        let edits = edits(item)?;
        for edit in &edits {
            let end = match edit {
                RewriteEdit::Insert { offset, .. } => *offset,
                RewriteEdit::Replace { end, .. } | RewriteEdit::Delete { end, .. } => *end,
            };
            if end > source_len {
                bail!("native edit outside source")
            }
        }
        outcome.fixes.push(AnalyzerFix { diagnostic_index, title: string(item, "title")?, edits })
    }
    Ok(outcome)
}

#[doc(hidden)]
pub fn native_attribute_result(type_id: &str, value: &Value, schema: &[u8]) -> Result<GeneratorOutcome> {
    exact_fields(value, &["declarations"])?;
    let correspondence = SyntaxCorrespondence::read(schema)?;
    let mut outcome = GeneratorOutcome { compiled_metadata: Vec::new(), type_id: type_id.into(), ..Default::default() };
    for declaration in array(value, "declarations")? {
        let projected = correspondence.to_rust("SpannedAttributeDeclaration", declaration)?;
        let item = serde_json::json!({"node":{"AttributeDeclaration":projected.clone()},"span":projected["span"],"id":projected["id"]});
        outcome.typed_items.push(serde_json::from_value(item)?);
    }
    Ok(outcome)
}

#[cfg(test)]
mod typed_result_tests {
    use super::*;
    #[test]
    fn v06_native_result_preserves_complete_source_function_wrapper() {
        let parsed = crate::services::parse_program_with_source_name_and_diagnostics(
            "Generated.bd",
            "pub unit Generated() { return; }",
        )
        .unwrap();
        assert!(!parsed.recovered);
        assert!(parsed.diagnostics.is_empty());
        let crate::syntax::Node::Function(definition) = &parsed.program.node.items[0].node else {
            panic!("real parsed function")
        };
        let schema = include_bytes!("../../../../corelib/packages/compiler-sdk/src/Beskid/Syntax/syntax.schema.json");
        let rust = serde_json::to_value(definition).unwrap();
        let source = SyntaxCorrespondence::read(schema).unwrap().to_source("SpannedFunctionDefinition", &rust).unwrap();
        let result = native_generator_result(
            "Fixture.Generator",
            &serde_json::json!({"items":[{"FunctionDefinition":{"definition":source}}],"targetBindings":[],"templateBindings":[]}),
            schema,
        )
        .unwrap();
        let crate::syntax::Node::Function(actual) = &result.typed_items[0].node else {
            panic!("typed function contribution")
        };
        assert_eq!(serde_json::to_value(actual).unwrap(), rust);
        assert!(result.provenance.is_empty(), "lineage is issued by the host arena, never decoded");
    }

    /// The retired source-text contribution field, and any other field outside the typed SDK
    /// contract, rejects the whole result; a missing typed field rejects it too.
    #[test]
    fn v06_native_generator_result_rejects_source_text_and_foreign_fields() {
        let schema = include_bytes!("../../../../corelib/packages/compiler-sdk/src/Beskid/Syntax/syntax.schema.json");
        let text = serde_json::json!({
            "items": [], "targetBindings": [], "templateBindings": [],
            "codeOutputs": [{"modulePath": "Generated", "fileName": "Generated", "body": {"language": "beskid", "body": "pub unit Generated() { return; }"}}]
        });
        let error = native_generator_result("Fixture.Generator", &text, schema).unwrap_err().to_string();
        assert!(error.contains("codeOutputs"), "{error}");
        let missing = serde_json::json!({"items": [], "targetBindings": []});
        assert!(native_generator_result("Fixture.Generator", &missing, schema).is_err());
        let empty = serde_json::json!({"items": [], "targetBindings": [], "templateBindings": []});
        assert!(native_generator_result("Fixture.Generator", &empty, schema).unwrap().typed_items.is_empty());
        let attribute = serde_json::json!({"declarations": [], "codeOutputs": []});
        assert!(native_attribute_result("Fixture.Attributes", &attribute, schema).is_err());
    }
}
