//! Concrete typed construction through generated SDK source, not host object casts.
use super::model::{FieldMirror, ParsedType, TypeKind, VariantShape};
use crate::syntax_helpers::HelperPaths;
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};
const PREFIX: &str = "Beskid.Syntax.Nodes";

/// Transfer class of a generated SDK type (`language-meta--memory-model--memory-and-references`):
/// primitives and `string` are Shared, an array of a Transferable element is Moved, an enum takes
/// the strongest class of its payloads, and the unmarked node records are Never.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Class {
    Shared,
    Moved,
    Never,
}

struct Classes<'a> {
    declarations: &'a BTreeMap<String, ParsedType>,
    helpers: &'a HelperPaths,
}

impl Classes<'_> {
    fn of(&self, ty: &str, visiting: &mut Vec<String>) -> Class {
        if let Some(element) = ty.strip_suffix("[]") {
            return if self.of(element, visiting) == Class::Never { Class::Never } else { Class::Moved };
        }
        if matches!(
            ty,
            "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "f32" | "f64" | "bool" | "char" | "word" | "unit" | "string"
        ) {
            return Class::Shared;
        }
        let name = ty.strip_prefix(PREFIX).and_then(|rest| rest.strip_prefix('.')).unwrap_or(ty);
        // A recursive payload is not proven Transferable here; Never keeps the parameter borrowed.
        if visiting.iter().any(|seen| seen == name) {
            return Class::Never;
        }
        visiting.push(name.to_owned());
        let class = if let Some(element) = self.helpers.opt_helpers.get(name) {
            self.of(element, visiting).max(Class::Shared)
        } else if let Some(declaration) = self.declarations.get(name).filter(|declaration| declaration.kind == TypeKind::Enum) {
            declaration
                .variants
                .iter()
                .flat_map(|entry| match &entry.shape {
                    VariantShape::Unit => &[][..],
                    VariantShape::Tuple(values) | VariantShape::Struct(values) => values.as_slice(),
                })
                .map(|field| self.of(&field.beskid_ty, visiting))
                .max()
                .unwrap_or(Class::Shared)
        } else {
            Class::Never
        };
        visiting.pop();
        class
    }

    /// A Moved parameter is declared `move`: the factory stores it into the new node, and a
    /// borrowed value cannot be stored (E1241).
    fn parameter(&self, ty: &str, name: &str) -> String {
        if self.of(ty, &mut Vec::new()) == Class::Moved { format!("move {ty} {name}") } else { format!("{ty} {name}") }
    }
}

fn emit(
    classes: &Classes<'_>,
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
    let parameters = fields.iter().map(|(ty, name)| classes.parameter(ty, name)).collect::<Vec<_>>().join(", ");
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
    classes: &Classes<'_>,
    source: &mut String,
    names: &mut BTreeSet<String>,
    ty: &str,
    fields: Vec<(String, String)>,
) -> io::Result<()> {
    let result = format!("{PREFIX}.{ty}");
    let assignments = fields.iter().map(|(_, name)| format!("{name}: {name}")).collect::<Vec<_>>().join(", ");
    let expression = format!("{result} {{ {assignments} }}");
    emit(classes, source, names, &format!("{ty}Value"), &result, &fields, &expression)
}
fn variant(
    classes: &Classes<'_>,
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
    emit(classes, source, names, &format!("{ty}{}Value", factory_name_segment(variant)?), &result, &fields, &expression)
}
/// The PascalCase factory-name segment of a variant. A variant whose Beskid name escapes a
/// reserved word with a leading underscore (`Type::_This`) keeps that spelling in the constructed
/// expression, but the generated callable is named without it (`TypeThisValue`), so every factory
/// is a PascalCase identifier.
fn factory_name_segment(variant: &str) -> io::Result<&str> {
    let segment = variant.trim_start_matches('_');
    if !segment.starts_with(|first: char| first.is_ascii_uppercase()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("native SDK variant {variant} has no PascalCase factory name"),
        ));
    }
    Ok(segment)
}

pub(super) fn native_syntax_factories(
    declarations: &BTreeMap<String, ParsedType>,
    helpers: &HelperPaths,
) -> io::Result<String> {
    let mut source = String::from("// Generated typed SDK constructors. Do not hand-edit.\n\n");
    let mut names = BTreeSet::new();
    let classes = Classes { declarations, helpers };
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
            TypeKind::Struct => record(&classes, &mut source, &mut names, name, fields(&declaration.fields)?)?,
            TypeKind::Enum => {
                for entry in &declaration.variants {
                    let payload = match &entry.shape {
                        VariantShape::Unit => Vec::new(),
                        VariantShape::Tuple(values) | VariantShape::Struct(values) => fields(values)?,
                    };
                    variant(&classes, &mut source, &mut names, name, &entry.name, payload)?;
                }
            }
        }
    }
    for (name, element) in &helpers.list_helpers {
        record(&classes, &mut source, &mut names, name, vec![(format!("{element}[]"), "items".into())])?;
    }
    for (name, element) in &helpers.opt_helpers {
        variant(&classes, &mut source, &mut names, name, "None", Vec::new())?;
        variant(&classes, &mut source, &mut names, name, "Some", vec![(element.clone(), "payload".into())])?;
    }
    for (name, element) in &helpers.spanned_helpers {
        record(
            &classes,
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
        &classes,
        &mut source,
        &mut names,
        "NodeSpan",
        ["start", "end", "lineStart", "columnStart", "lineEnd", "columnEnd"]
            .into_iter()
            .map(|name| ("u64".into(), name.into()))
            .collect(),
    )?;
    record(
        &classes,
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
