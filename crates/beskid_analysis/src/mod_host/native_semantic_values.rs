//! Explicit source SDK DTO construction from issuer-returned owned semantic facts.
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
fn field<'a>(v: &'a Value, k: &str) -> Result<&'a Value> {
    v.get(k).with_context(|| format!("semantic fact {k} absent"))
}
fn map_array(v: &Value, f: impl Fn(&Value) -> Result<Value>) -> Result<Value> {
    Ok(Value::Array(v.as_array().context("semantic array fact absent")?.iter().map(f).collect::<Result<Vec<_>>>()?))
}
fn option(v: &Value, f: impl FnOnce(&Value) -> Result<Value>) -> Result<Value> {
    Ok(if v.is_null() { json!("None") } else { json!({"Some":{"value":f(v)?}}) })
}
fn span(v: &Value) -> Result<Value> {
    let start = field(v, "line_col_start")?.as_array().context("semantic span start absent")?;
    let end = field(v, "line_col_end")?.as_array().context("semantic span end absent")?;
    if start.len() != 2 || end.len() != 2 {
        bail!("semantic span arity mismatch");
    }
    Ok(
        json!({"start":field(v,"start")?,"end":field(v,"end")?,"lineStart":start[0],"columnStart":start[1],"lineEnd":end[0],"columnEnd":end[1]}),
    )
}
fn declaration(v: &Value) -> Result<Value> {
    Ok(
        json!({"sourceUnit":field(v,"source_unit")?,"generation":field(v,"generation")?,"node":field(v,"node")?,"span":span(field(v,"span")?)?}),
    )
}
fn ty(v: &Value) -> Result<Value> {
    let object = v.as_object().context("semantic field type absent")?;
    if object.len() != 1 {
        bail!("semantic field type ambiguous");
    }
    let (kind, payload) = object.iter().next().unwrap();
    Ok(match kind.as_str() {
        "Scalar" => json!({"Scalar":{"primitive":payload}}),
        "Nominal" => json!({"Nominal":{"handle":{"token":payload}}}),
        "Array" => json!({"Array":{"element":[ty(payload)?]}}),
        _ => bail!("unknown issuer semantic type"),
    })
}
fn member(v: &Value) -> Result<Value> {
    Ok(
        json!({"name":field(v,"name")?,"declaration":declaration(field(v,"declaration")?)?,"ty":ty(field(v,"ty")?)?,"ownership":field(v,"ownership")?}),
    )
}
fn variant(v: &Value) -> Result<Value> {
    Ok(
        json!({"name":field(v,"name")?,"ordinal":field(v,"ordinal")?,"declaration":declaration(field(v,"declaration")?)?,"fields":map_array(field(v,"fields")?,member)?}),
    )
}
fn package(v: &Value) -> Result<Value> {
    let source = field(v, "source")?;
    let source = if let Some(row) = source.get("Registry") {
        json!({"Registry":{"origin":field(row,"registry")?,"artifactDigest":field(row,"artifact_digest")?}})
    } else {
        source.clone()
    };
    Ok(
        json!({"name":field(v,"package_name")?,"version":field(v,"version")?,"source":source,"sourceDigest":field(v,"source_digest")?}),
    )
}
pub(crate) fn source_shape(v: &Value) -> Result<Value> {
    let body = field(v, "body")?;
    let body = if let Some(record) = body.get("Record") {
        json!({"Record":{"fields":map_array(field(record,"fields")?,member)?}})
    } else if let Some(enumeration) = body.get("Enum") {
        json!({"Enum":{"variants":map_array(field(enumeration,"variants")?,variant)?}})
    } else {
        bail!("semantic shape body lacks exact discriminator");
    };
    Ok(json!({"name":field(v,"name")?,"declarationIdentity":field(v,"declaration_identity")?,
 "packageDeclaration":option(field(v,"package_declaration")?,|p|Ok(json!({"sourcePath":field(p,"source_path")?,"lexicalPath":field(p,"lexical_path")?})))?,
 "declaration":declaration(field(v,"declaration")?)?,"typeArguments":map_array(field(v,"type_arguments")?,ty)?,"body":body,
 "package":option(field(v,"package")?,package)?}))
}
pub(crate) fn semantic_result(response: &Value, shape: bool) -> Result<Value> {
    if let Some(error) = response.get("error") {
        return Ok(json!({"Error":{"error":{"code":"ModSemantic","message":error}}}));
    }
    let value =
        if shape { source_shape(field(response, "shape")?)? } else { json!({"token":field(response,"handle")?}) };
    Ok(json!({"Ok":{"value":value}}))
}

pub(crate) fn catchall_result(response: &Value) -> Result<Value> {
    if let Some(error) = response.get("error") {
        return Ok(json!({"Error":{"error":{"code":"ModCatchall","message":error}}}));
    }
    let claim = field(response, "catchall")?;
    Ok(json!({"Ok":{"value":{"token":field(claim,"token")?,"owner":{"token":field(claim,"owner")?},
        "field":declaration(field(claim,"field")?)?,"map":{"token":field(claim,"map")?},"value":ty(field(claim,"value")?)?}}}))
}

pub(crate) fn function_result(response: &Value) -> Result<Value> {
    if let Some(error) = response.get("error") {
        return Ok(json!({"Error":{"error":{"code":"ModFunction","message":error}}}));
    }
    let signature = field(response, "function")?;
    Ok(json!({"Ok":{"value":{"declaration":declaration(field(signature,"declaration")?)?,
        "genericCount":field(signature,"generic_count")?,"parameters":map_array(field(signature,"parameters")?,ty)?,
        "result":ty(field(signature,"result")?)?}}}))
}

pub(crate) fn serializable_result(response: &Value) -> Result<Value> {
    if let Some(error) = response.get("error") {
        return Ok(json!({"Error":{"error":{"code":"ModSerializable","message":error}}}));
    }
    if field(response, "serializable")? != &Value::Bool(true) {
        bail!("serialization gate response lacks exact admission");
    }
    Ok(json!({"Ok":{"value":null}}))
}
