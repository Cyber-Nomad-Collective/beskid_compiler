//! Concrete typed construction through generated SDK source, not host object casts.
use super::model::{FieldMirror, ParsedType, TypeKind, VariantShape};
use crate::syntax_helpers::HelperPaths;
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};
const PREFIX: &str = "Beskid.Syntax.Nodes";

fn emit(
    source: &mut String,
    names: &mut BTreeSet<String>,
    name: &str,
    result: &str,
    fields: &[(String, String)],
    expression: &str,
) -> io::Result<()> {
    if !names.insert(name.to_owned()) {
        return Err(io::Error::new(io::ErrorKind::InvalidData, format!("ambiguous native SDK constructor {name}")));
    }
    let parameters = fields.iter().map(|(ty, name)| format!("{ty} {name}")).collect::<Vec<_>>().join(", ");
    source.push_str(&format!("pub {result} {name}({parameters}) {{\n    return {expression};\n}}\n\n"));
    Ok(())
}
fn fields(fields: &[FieldMirror]) -> io::Result<Vec<(String, String)>> {
    fields
        .iter()
        .map(|field| {
            if field.stub_note.is_some() || field.beskid_ty.contains("ReflectStub") {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "native SDK constructor has an opaque field"));
            }
            Ok((field.beskid_ty.clone(), field.name.clone()))
        })
        .collect()
}
fn record(
    source: &mut String,
    names: &mut BTreeSet<String>,
    ty: &str,
    fields: Vec<(String, String)>,
) -> io::Result<()> {
    let result = format!("{PREFIX}.{ty}");
    let assignments = fields.iter().map(|(_, name)| format!("{name}: {name}")).collect::<Vec<_>>().join(", ");
    let expression = format!("{result} {{ {assignments} }}");
    emit(source, names, &format!("{ty}Value"), &result, &fields, &expression)
}
fn variant(
    source: &mut String,
    names: &mut BTreeSet<String>,
    ty: &str,
    variant: &str,
    fields: Vec<(String, String)>,
) -> io::Result<()> {
    let result = format!("{PREFIX}.{ty}");
    let expression = if fields.is_empty() {
        format!("{result}::{variant}")
    } else {
        format!("{result}::{variant}({})", fields.iter().map(|(_, name)| name.as_str()).collect::<Vec<_>>().join(", "))
    };
    emit(source, names, &format!("{ty}{variant}Value"), &result, &fields, &expression)
}

pub(super) fn native_syntax_factories(
    declarations: &BTreeMap<String, ParsedType>,
    helpers: &HelperPaths,
) -> io::Result<String> {
    let mut source =
        String::from("// Generated typed SDK constructors. Do not hand-edit.\nuse Beskid.Syntax.Nodes;\n\n");
    let mut names = BTreeSet::new();
    for (name, declaration) in declarations {
        if crate::syntax_traversal::is_host_only_type(name) {
            continue;
        }
        if !declaration.type_param_names.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "native SDK constructor has unapplied generic fields",
            ));
        }
        match declaration.kind {
            TypeKind::Struct => record(&mut source, &mut names, name, fields(&declaration.fields)?)?,
            TypeKind::Enum => {
                for entry in &declaration.variants {
                    let payload = match &entry.shape {
                        VariantShape::Unit => Vec::new(),
                        VariantShape::Tuple(values) | VariantShape::Struct(values) => fields(values)?,
                    };
                    variant(&mut source, &mut names, name, &entry.name, payload)?;
                }
            }
        }
    }
    for (name, element) in &helpers.list_helpers {
        record(&mut source, &mut names, name, vec![(format!("{element}[]"), "items".into())])?;
    }
    for (name, element) in &helpers.opt_helpers {
        variant(&mut source, &mut names, name, "None", Vec::new())?;
        variant(&mut source, &mut names, name, "Some", vec![(element.clone(), "payload".into())])?;
    }
    for (name, element) in &helpers.spanned_helpers {
        record(
            &mut source,
            &mut names,
            name,
            vec![
                (element.clone(), "node".into()),
                (format!("{PREFIX}.NodeSpan"), "span".into()),
                ("u32".into(), "id".into()),
            ],
        )?;
    }
    record(
        &mut source,
        &mut names,
        "NodeSpan",
        ["start", "end", "lineStart", "columnStart", "lineEnd", "columnEnd"]
            .into_iter()
            .map(|name| ("u64".into(), name.into()))
            .collect(),
    )?;
    record(
        &mut source,
        &mut names,
        "NodeRef",
        vec![
            ("string".into(), "sourceUnit".into()),
            ("u64".into(), "invocationIssuer".into()),
            ("u64".into(), "syntaxGenerationId".into()),
            ("u32".into(), "nodeId".into()),
        ],
    )?;
    Ok(source)
}
