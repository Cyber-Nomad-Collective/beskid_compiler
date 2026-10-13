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
                if fields.is_empty() {
                    // A field-less syntax record (`BreakStatement`, `ContinueStatement`) is a Rust
                    // unit struct, serialized as `null`; its SDK value is an empty record.
                    return if forward {
                        if !value.is_null() {
                            bail!("syntax unit record {name} carries data");
                        }
                        Ok(json!({}))
                    } else {
                        if value.as_object().is_none_or(|object| !object.is_empty()) {
                            bail!("SDK unit record {name} carries fields");
                        }
                        Ok(Value::Null)
                    };
                }
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
    /// `break;`/`continue;` round-trip between the Rust unit structs and empty SDK records.
    #[test]
    fn field_less_syntax_records_round_trip_as_rust_unit_structs() {
        let schema = schema();
        for name in ["BreakStatement", "ContinueStatement"] {
            assert_eq!(schema.to_source(name, &Value::Null).unwrap(), json!({}));
            assert_eq!(schema.to_rust(name, &json!({})).unwrap(), Value::Null);
            assert!(schema.to_rust(name, &json!({"extra":1})).is_err());
            assert!(schema.to_source(name, &json!({})).is_err());
        }
        let parsed = crate::services::parse_program_with_source_name("Loop.bd", "unit Loop() { while true { break; } }")
            .expect("parse loop");
        let crate::syntax::Node::Function(function) = &parsed.node.items[0].node else { panic!("function") };
        let rust = serde_json::to_value(function).unwrap();
        let source = schema.to_source("SpannedFunctionDefinition", &rust).unwrap();
        assert_eq!(schema.to_rust("SpannedFunctionDefinition", &source).unwrap(), rust);
    }
    #[test]
    fn native_syntax_correspondence_rejects_lost_float_payload() {
        assert!(schema().to_source("f64", &Value::Null).is_err());
    }
    fn identifiers(count: usize) -> Value {
        Value::Array(
            (0..count)
                .map(|index| {
                    json!({"node":{"name":"i"},"span":{"start":0,"end":1,"line_col_start":[1,1],"line_col_end":[1,2]},"id":index})
                })
                .collect(),
        )
    }

    /// A transported syntax list holds at most 65536 elements in either direction.
    #[test]
    fn transport_list_budget_is_exact_at_65536_elements() {
        let schema = schema();
        let exact = schema.to_source("SpannedIdentifierList", &identifiers(65536)).expect("65536 elements fit");
        assert_eq!(schema.to_rust("SpannedIdentifierList", &exact).unwrap().as_array().unwrap().len(), 65536);
        assert!(schema.to_source("SpannedIdentifierList", &identifiers(65537)).is_err());
        let mut over = exact;
        let first = over["items"][0].clone();
        over["items"].as_array_mut().unwrap().push(first);
        assert!(schema.to_rust("SpannedIdentifierList", &over).is_err(), "SDK-to-host direction is bounded too");
    }

    /// Correspondence recursion is bounded at depth 128: a deeply nested expression is
    /// rejected in both directions rather than exhausting the stack.
    #[test]
    fn transport_depth_budget_rejects_nesting_past_128() {
        let schema = schema();
        let span = json!({"start":0,"end":1,"line_col_start":[1,1],"line_col_end":[1,2]});
        let nested = |depth: usize| -> Value {
            let mut expression = json!({
                "node":{"Literal":{"node":{"literal":{"node":{"Integer":"1"},"span":span,"id":1}},"span":span,"id":2}},
                "span":span,"id":3
            });
            for level in 0..depth {
                expression = json!({
                    "node":{"Grouped":{"node":{"expr":expression},"span":span,"id":4 + 2 * level}},
                    "span":span,"id":5 + 2 * level
                });
            }
            expression
        };
        let shallow = schema.to_source("SpannedExpression", &nested(4)).expect("shallow nesting is transported");
        assert_eq!(schema.to_rust("SpannedExpression", &shallow).unwrap(), nested(4));
        let deep = nested(200);
        let error = schema.to_source("SpannedExpression", &deep).unwrap_err().to_string();
        assert!(error.contains("recursion budget"), "{error}");
        let mut source = shallow;
        for level in 0..200 {
            source = json!({
                "node":{"Grouped":{"payload":{"node":{"expr":source},"span":schema.to_source("NodeSpan",&span).unwrap(),"id":level}}},
                "span":schema.to_source("NodeSpan",&span).unwrap(),"id":level
            });
        }
        assert!(schema.to_rust("SpannedExpression", &source).is_err(), "SDK-to-host direction is bounded too");
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
