//! Lossless structural correspondence between canonical Rust AST serialization and SDK values.
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

pub(crate) struct SyntaxCorrespondence {
    schema: Value,
}
impl SyntaxCorrespondence {
    /// Bytes must be the producer-pinned schema from the qualified artifact inventory.
    pub fn read(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > 16 * 1024 * 1024 {
            bail!("syntax correspondence schema too large");
        }
        let schema: Value = serde_json::from_slice(bytes)?;
        if schema["schemaVersion"] != 2 || schema["module"] != "Beskid.Syntax.Nodes" {
            bail!("syntax correspondence schema identity mismatch");
        }
        Ok(Self { schema })
    }
    pub fn to_source(&self, name: &str, value: &Value) -> Result<Value> {
        self.map(name, value, true, 0)
    }
    pub fn to_rust(&self, name: &str, value: &Value) -> Result<Value> {
        self.map(name, value, false, 0)
    }
    fn map(&self, name: &str, value: &Value, forward: bool, depth: usize) -> Result<Value> {
        if depth > 128 {
            bail!("syntax correspondence recursion budget exceeded");
        }
        let name = name.strip_prefix("Beskid.Syntax.Nodes.").unwrap_or(name);
        let Some(body) = self.schema["types"].get(name).and_then(|ty| ty.get("body")) else {
            // Primitive values keep their width and identity. Nonfinite floats must
            // arrive in the explicit bit-preserving transport, never JSON null.
            if matches!(name, "f32" | "f64") && value.is_null() {
                bail!("float AST projection lost IEEE payload");
            }
            return Ok(value.clone());
        };
        let recurse = |ty: &str, v: &Value| self.map(ty, v, forward, depth + 1);
        Ok(match body["kind"].as_str().context("syntax schema body kind absent")? {
            "record" => {
                let fields = body["fields"].as_array().context("syntax schema fields absent")?;
                let object = value.as_object().context("syntax record requires object")?;
                if object.len() != fields.len() {
                    bail!("syntax record field count mismatch for {name}");
                }
                let mut result = serde_json::Map::new();
                for field in fields {
                    let rust = field["rustName"].as_str().context("syntax Rust field absent")?;
                    let source = field["beskidName"].as_str().context("syntax SDK field absent")?;
                    let ty = field["type"].as_str().context("syntax field type absent")?;
                    result.insert(
                        if forward { source } else { rust }.into(),
                        recurse(ty, object.get(if forward { rust } else { source }).context("syntax field missing")?)?,
                    );
                }
                Value::Object(result)
            }
            "spanned" => {
                let node = recurse(body["element"].as_str().context("spanned element absent")?, &value["node"])?;
                let span = recurse("NodeSpan", &value["span"])?;
                if value.get("id").is_none() {
                    bail!("syntax node ID absent");
                }
                json!({"node":node,"span":span,"id":value["id"]})
            }
            "source-span" => {
                if forward {
                    let start = value["line_col_start"].as_array().context("source span start absent")?;
                    let end = value["line_col_end"].as_array().context("source span end absent")?;
                    if start.len() != 2 || end.len() != 2 {
                        bail!("source span position arity mismatch");
                    }
                    json!({"start":value["start"],"end":value["end"],"lineStart":start[0],"columnStart":start[1],"lineEnd":end[0],"columnEnd":end[1]})
                } else {
                    json!({"start":value["start"],"end":value["end"],"line_col_start":[value["lineStart"],value["columnStart"]],"line_col_end":[value["lineEnd"],value["columnEnd"]]})
                }
            }
            "optional" => {
                let ty = body["element"].as_str().context("optional element absent")?;
                if forward {
                    if value.is_null() { json!("None") } else { json!({"Some":{"payload":recurse(ty,value)?}}) }
                } else if value == "None" {
                    Value::Null
                } else {
                    recurse(ty, value.get("Some").and_then(|v| v.get("payload")).context("optional payload absent")?)?
                }
            }
            "list" => {
                if body["storage"] != "managed-array-record" || body["field"] != "items" {
                    bail!("syntax list has obsolete storage correspondence");
                }
                let ty = body["element"].as_str().context("list element absent")?;
                let elements = if forward {
                    value.as_array()
                } else {
                    let object = value.as_object().context("SDK list requires canonical record")?;
                    if object.len() != 1 {
                        bail!("SDK list has foreign fields");
                    }
                    object.get("items").and_then(Value::as_array)
                }
                .context("syntax list items absent")?;
                if elements.len() > 65536 {
                    bail!("syntax list count budget exceeded");
                }
                let items = elements.iter().map(|element| recurse(ty, element)).collect::<Result<Vec<_>>>()?;
                if forward { json!({"items":items}) } else { Value::Array(items) }
            }
            "enum" => {
                let (tag, payload) = if let Some(tag) = value.as_str() {
                    (tag, Value::Null)
                } else {
                    let obj = value.as_object().context("syntax enum requires single tagged value")?;
                    if obj.len() != 1 {
                        bail!("syntax enum tag ambiguous");
                    }
                    let (tag, payload) = obj.iter().next().unwrap();
                    (tag.as_str(), payload.clone())
                };
                let variants = body["variants"].as_array().context("syntax variants absent")?;
                let variant = variants
                    .iter()
                    .find(|v| v[if forward { "rustName" } else { "beskidName" }] == tag)
                    .context("syntax variant unknown")?;
                let out = variant[if forward { "beskidName" } else { "rustName" }]
                    .as_str()
                    .context("syntax variant output name absent")?;
                let fields = variant["fields"].as_array().context("syntax variant fields absent")?;
                if fields.is_empty() {
                    if !payload.is_null() {
                        bail!("unit syntax variant carries payload");
                    }
                    json!(out)
                } else {
                    let tuple = variant["kind"] == "tuple";
                    let mut converted = Vec::new();
                    let mut object = serde_json::Map::new();
                    for (index, field) in fields.iter().enumerate() {
                        let from = field[if forward { "rustName" } else { "beskidName" }]
                            .as_str()
                            .context("variant field name absent")?;
                        let to = field[if forward { "beskidName" } else { "rustName" }]
                            .as_str()
                            .context("variant field name absent")?;
                        let raw = if forward && tuple {
                            if fields.len() == 1 {
                                &payload
                            } else {
                                payload.get(index).context("tuple payload missing")?
                            }
                        } else {
                            payload.get(from).context("named payload missing")?
                        };
                        let v = recurse(field["type"].as_str().context("variant field type absent")?, raw)?;
                        converted.push(v.clone());
                        object.insert(to.into(), v);
                    }
                    let payload = if !forward && tuple {
                        if converted.len() == 1 { converted.remove(0) } else { Value::Array(converted) }
                    } else {
                        Value::Object(object)
                    };
                    json!({out:payload})
                }
            }
            "issued-node-reference" => bail!("syntax node handles require current issuer projection"),
            _ => bail!("unsupported canonical syntax correspondence body"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn schema() -> SyntaxCorrespondence {
        SyntaxCorrespondence::read(include_bytes!(
            "../../../../corelib/packages/compiler-sdk/src/Beskid/Syntax/syntax.schema.json"
        ))
        .unwrap()
    }
    #[test]
    fn native_syntax_correspondence_preserves_span_ids_optional_and_lists() {
        let schema = schema();
        let span = json!({"start":2,"end":9,"line_col_start":[1,3],"line_col_end":[2,4]});
        let source = schema.to_source("NodeSpan", &span).unwrap();
        assert_eq!(schema.to_rust("NodeSpan", &source).unwrap(), span);
        let empty = json!([]);
        let source = schema.to_source("SpannedExpressionList", &empty).unwrap();
        assert_eq!(source, json!({"items":[]}));
        assert_eq!(schema.to_rust("SpannedExpressionList", &source).unwrap(), empty);
        assert_eq!(schema.to_source("OptionalSpannedExpression", &Value::Null).unwrap(), json!("None"));
    }
    #[test]
    fn native_syntax_correspondence_rejects_lost_float_payload() {
        assert!(schema().to_source("f64", &Value::Null).is_err());
    }
    #[test]
    fn flat_sdk_list_preserves_256_identifiers_spans_and_ids() {
        let schema = schema();
        let original = Value::Array(
            (0..256)
                .map(|index| {
                    json!({
                        "node":{"name":format!("identifier{index}")},
                        "span":{"start":index*3,"end":index*3+2,
                            "line_col_start":[index+1,1],"line_col_end":[index+1,3]},
                        "id":index+1
                    })
                })
                .collect(),
        );
        let source = schema.to_source("SpannedIdentifierList", &original).unwrap();
        assert_eq!(source["items"].as_array().unwrap().len(), 256);
        // The actual framed JSON parser's nesting limit must accept a flat list.
        let bytes = serde_json::to_vec(&source).unwrap();
        let transported: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(schema.to_rust("SpannedIdentifierList", &transported).unwrap(), original);
    }
}
