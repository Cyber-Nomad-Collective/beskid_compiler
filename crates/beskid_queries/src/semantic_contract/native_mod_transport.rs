//! Current-generation source signatures for compiler-owned typed Mod adapters.
//! Native ABI pointer equality is never a source-type transport identity.
use super::*;
use beskid_analysis::syntax::{EnumDefinition, FieldKind, FunctionDefinition, MethodDefinition, Type, TypeDefinition};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NativeModTransportType {
    Scalar(SemanticTypeId),
    Array(Box<NativeModTransportType>),
    Nominal { declaration: AstNodeKey, arguments: Vec<NativeModTransportType> },
    Function { parameters: Vec<NativeModTransportType>, result: Box<NativeModTransportType> },
}
impl NativeModTransportType {
    pub fn abi_type(&self) -> SemanticTypeId {
        match self {
            Self::Scalar(ty) => *ty,
            _ => SemanticTypeId::POINTER,
        }
    }
}
#[derive(Debug, Clone)]
pub struct NativeModTransportSignature {
    declaration: AstNodeKey,
    parameters: Vec<NativeModTransportType>,
    result: NativeModTransportType,
}
impl NativeModTransportSignature {
    pub fn declaration(&self) -> AstNodeKey {
        self.declaration
    }
    pub fn parameters(&self) -> &[NativeModTransportType] {
        &self.parameters
    }
    pub fn result(&self) -> &NativeModTransportType {
        &self.result
    }
}
fn source_type(
    db: &dyn Db,
    context: AstNodeKey,
    ty: &Type,
    depth: usize,
) -> Result<NativeModTransportType, SemanticError> {
    source_type_environment(db, context, ty, depth, &std::collections::HashMap::new())
}
fn source_type_environment(
    db: &dyn Db,
    context: AstNodeKey,
    ty: &Type,
    depth: usize,
    environment: &std::collections::HashMap<String, NativeModTransportType>,
) -> Result<NativeModTransportType, SemanticError> {
    if depth > 128 {
        return Err(SemanticError::new("native Mod source type exceeds transport depth"));
    }
    Ok(match ty {
        Type::Primitive(_) => NativeModTransportType::Scalar(abi_type_from_syntax(db, context, ty)?),
        Type::Array(element) => NativeModTransportType::Array(Box::new(source_type_environment(
            db,
            context,
            &element.node,
            depth + 1,
            environment,
        )?)),
        Type::Complex(path) => {
            if let [segment] = path.node.segments.as_slice() {
                if segment.node.type_args.is_empty() {
                    if let Some(argument) = environment.get(&segment.node.name.node.name) {
                        return Ok(argument.clone());
                    }
                }
            }
            let declaration = layouts::resolve_type_declaration(db, context, &path.node)
                .ok_or_else(|| SemanticError::new("native Mod source nominal type is unresolved"))?;
            let syntax = db
                .syntax_unit(declaration.unit)
                .filter(|syntax| syntax.accepts_key(db, declaration))
                .ok_or_else(|| SemanticError::new("native Mod source nominal type is stale"))?;
            let owner = db
                .syntax_unit(context.unit)
                .filter(|syntax| syntax.accepts_key(db, context))
                .ok_or_else(|| SemanticError::new("native Mod source signature is stale"))?;
            if syntax.project(db) != owner.project(db) {
                return Err(SemanticError::new("native Mod source nominal type is foreign"));
            }
            let arguments = path
                .node
                .segments
                .iter()
                .flat_map(|segment| segment.node.type_args.iter())
                .map(|argument| source_type_environment(db, context, &argument.node, depth + 1, environment))
                .collect::<Result<Vec<_>, _>>()?;
            NativeModTransportType::Nominal { declaration, arguments }
        }
        Type::Function { parameters, return_type } => NativeModTransportType::Function {
            parameters: parameters
                .iter()
                .map(|parameter| source_type_environment(db, context, &parameter.node, depth + 1, environment))
                .collect::<Result<Vec<_>, _>>()?,
            result: Box::new(source_type_environment(db, context, &return_type.node, depth + 1, environment)?),
        },
        Type::This => {
            let receiver = method_this_type(db, context)
                .ok_or_else(|| SemanticError::new("native Mod This type lacks current receiver authority"))?;
            return source_type_environment(db, context, &receiver, depth + 1, environment);
        }
        Type::Associated { .. } => {
            return Err(SemanticError::new("native Mod associated type requires a concrete implementation witness"));
        }
    })
}
/// Issue a concrete signature only from the current registered declaration.
/// Unapplied function templates never become native adapter callables.
pub fn native_mod_transport_signature(
    db: &dyn Db,
    key: AstNodeKey,
) -> SemanticQueryResult<NativeModTransportSignature> {
    let Some(syntax) = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key)) else { return Ok(None) };
    let index = syntax.syntax_index(db);
    let Some(node) = index.node_at(syntax.expanded_program(db), key.node) else { return Ok(None) };
    let (parameters, result, receiver) = if let Some(function) = node.of::<FunctionDefinition>() {
        if !function.generics.is_empty() {
            return Ok(None);
        }
        (&function.parameters, &function.return_type, None)
    } else if let Some(method) = node.of::<MethodDefinition>() {
        (&method.parameters, &method.return_type, Some(&method.receiver_type.node))
    } else {
        return Ok(None);
    };
    let mut types = Vec::with_capacity(parameters.len() + usize::from(receiver.is_some()));
    if let Some(receiver) = receiver {
        types.push(source_type(db, key, receiver, 0)?);
    }
    for parameter in parameters {
        types.push(source_type(db, key, &parameter.node.ty.node, 0)?);
    }
    let result = match result {
        Some(result) => source_type(db, key, &result.node, 0)?,
        None => NativeModTransportType::Scalar(SemanticTypeId::UNIT),
    };
    Ok(Some(NativeModTransportSignature { declaration: key, parameters: types, result }))
}

#[derive(Debug, Clone)]
pub struct NativeModTransportField {
    name: String,
    ty: NativeModTransportType,
}
impl NativeModTransportField {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn ty(&self) -> &NativeModTransportType {
        &self.ty
    }
}
#[derive(Debug, Clone)]
pub struct NativeModTransportVariant {
    name: String,
    fields: Vec<NativeModTransportField>,
}
impl NativeModTransportVariant {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn fields(&self) -> &[NativeModTransportField] {
        &self.fields
    }
}
#[derive(Debug, Clone)]
pub enum NativeModTransportBody {
    Record(Vec<NativeModTransportField>),
    Enum(Vec<NativeModTransportVariant>),
}
#[derive(Debug, Clone)]
pub struct NativeModTransportNominal {
    declaration: AstNodeKey,
    name: String,
    body: NativeModTransportBody,
}
impl NativeModTransportNominal {
    pub fn declaration(&self) -> AstNodeKey {
        self.declaration
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn body(&self) -> &NativeModTransportBody {
        &self.body
    }
}
/// Concrete field/variant projection retains applied argument identities, not ABI pointers.
pub fn native_mod_transport_nominal(
    db: &dyn Db,
    context: AstNodeKey,
    ty: &NativeModTransportType,
) -> SemanticQueryResult<NativeModTransportNominal> {
    let NativeModTransportType::Nominal { declaration, arguments } = ty else { return Ok(None) };
    let owner = db
        .syntax_unit(context.unit)
        .filter(|syntax| syntax.accepts_key(db, context))
        .ok_or_else(|| SemanticError::new("native Mod transport owner is stale"))?;
    let syntax = db
        .syntax_unit(declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, *declaration) && syntax.project(db) == owner.project(db))
        .ok_or_else(|| SemanticError::new("native Mod transport declaration is stale or foreign"))?;
    let node = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), declaration.node)
        .ok_or_else(|| SemanticError::new("native Mod transport declaration is absent"))?;
    let (name, generics) = if let Some(record) = node.of::<TypeDefinition>() {
        (&record.name, &record.generics)
    } else if let Some(enumeration) = node.of::<EnumDefinition>() {
        (&enumeration.name, &enumeration.generics)
    } else {
        return Ok(None);
    };
    if generics.len() != arguments.len() {
        return Err(SemanticError::new("native Mod transport nominal has unapplied arguments"));
    }
    let environment = generics
        .iter()
        .zip(arguments)
        .map(|(parameter, argument)| (parameter.node.name.clone(), argument.clone()))
        .collect::<std::collections::HashMap<_, _>>();
    let fields = |fields:&[beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Field>]| -> Result<Vec<NativeModTransportField>,SemanticError> {
        if fields.len()>65536 {return Err(SemanticError::new("native Mod transport exceeds field budget"))}
        fields.iter().map(|field| {
            if field.node.kind!=FieldKind::Value {return Err(SemanticError::new("native Mod transport field requires explicit event/injection authority"))}
            Ok(NativeModTransportField {name:field.node.name.node.name.clone(),ty:source_type_environment(db,*declaration,&field.node.ty.node,0,&environment)?})
        }).collect()
    };
    let body = if let Some(record) = node.of::<TypeDefinition>() {
        NativeModTransportBody::Record(fields(&record.fields)?)
    } else {
        let enumeration = node.of::<EnumDefinition>().unwrap();
        if enumeration.variants.len() > 65536 {
            return Err(SemanticError::new("native Mod transport exceeds variant budget"));
        }
        NativeModTransportBody::Enum(
            enumeration
                .variants
                .iter()
                .map(|variant| {
                    Ok(NativeModTransportVariant {
                        name: variant.node.name.node.name.clone(),
                        fields: fields(&variant.node.fields)?,
                    })
                })
                .collect::<Result<Vec<_>, SemanticError>>()?,
        )
    };
    Ok(Some(NativeModTransportNominal { declaration: *declaration, name: name.node.name.clone(), body }))
}

/// Exact sole array-literal element; source nodes remain behind the query boundary.
pub fn native_mod_single_array_element(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<AstNodeKey> {
    let Some(syntax) = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key)) else { return Ok(None) };
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    let Some(array) =
        index.node_at(program, key.node).and_then(|node| node.of::<beskid_analysis::syntax::ArrayLiteralExpression>())
    else {
        return Ok(None);
    };
    if array.elements.len() != 1 {
        return Ok(None);
    };
    Ok(index
        .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(&array.elements[0]))
        .map(|node| AstNodeKey { node, ..key }))
}
