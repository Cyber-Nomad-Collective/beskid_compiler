//! Host hooks (`startup`, `init`, `dispose`) and host/scope parameters as ordinary items of the
//! expanded syntax.
//!
//! A hook body is executable code with injected parameters. Instead of a second, hook-only
//! semantic path, expansion moves each hook body into a private top-level function and, when
//! the hook has parameters, a private argument type whose injected fields mirror the parameters
//! one to one. The argument type is registered in the hook's own container level, so the
//! ordinary composition graph resolves, orders and diagnoses every hook parameter exactly like a
//! field `inject`. The function takes the argument object itself (`__beskidServices`) and reads
//! the parameters its body names from it, so every hook function takes at most two arguments
//! (record and argument object), however many services it injects.
//!
//! Host parameters (`host App(string name)`) and scope parameters (`scope Visit(i32 id)`) become
//! a private value type per host or scope whose fields are the parameters. Each `launch App(a);`
//! and `with Visit(b) { }` whose argument count matches is rewritten to build that record in a
//! local just before the statement (`__BeskidHookHostArgs_App __beskidLaunchArgs_N =
//! __BeskidHookHostArgs_App { name: a };`), so the ordinary type checker checks every argument at
//! its own position. The hooks of that host (`startup`, global `init`/`dispose`) or scope take the
//! record as their first parameter and read the parameters they name from it. Lowering passes the
//! local to them. An argument count that does not match is left in place and composition reports
//! it (E1722 for `launch`, E1708 for `with`).
//!
//! The hook node stays in the host with its span (the composition plan keys hooks by it); its
//! parameters and body are emptied so that each source node exists exactly once in the tree.

use std::collections::{HashMap, HashSet};

use crate::syntax::{
    Block, ElseBranch, Expression, Field, FieldKind, FunctionDefinition, HostBodyItem, Identifier, LetStatement,
    MemberExpression, Node, Parameter, Path, PathExpression, PathSegment, PrimitiveType, Program,
    RegistrationLifetime, RegistryEntry, ScopeHook, ScopeHookKind, SpanInfo, Spanned, Statement,
    StructLiteralExpression, StructLiteralField, Type, TypeDefinition, Visibility,
};

/// Reserved name prefix of compiler-generated hook items. Source never names them.
pub const HOOK_ITEM_PREFIX: &str = "__BeskidHook";
/// Prefix of the local that holds the record of one `launch` statement's arguments.
pub const LAUNCH_ARGUMENTS_LOCAL_PREFIX: &str = "__beskidLaunchArgs_";
/// Prefix of the local that holds the record of one `with` statement's arguments.
pub const SCOPE_ARGUMENTS_LOCAL_PREFIX: &str = "__beskidScopeArgs_";
/// The leading parameter of a hook function that receives its host or scope record.
pub const ARGUMENTS_RECORD_PARAMETER: &str = "__beskidArgs";
/// The parameter of a hook function that receives its argument object (the injected services).
pub const SERVICES_PARAMETER: &str = "__beskidServices";

/// Whether `name` is a compiler-generated hook item: a hook function, a hook argument type, a
/// host or scope record type, or one of the generated locals and parameters that carry records.
pub fn is_hook_item_name(name: &str) -> bool {
    name.starts_with(HOOK_ITEM_PREFIX)
        || name.starts_with(LAUNCH_ARGUMENTS_LOCAL_PREFIX)
        || name.starts_with(SCOPE_ARGUMENTS_LOCAL_PREFIX)
        || name == ARGUMENTS_RECORD_PARAMETER
        || name == SERVICES_PARAMETER
}

/// Name of the function that holds the body of the hook declared at `span` in `host`.
pub fn hook_function_name(host: &str, kind: ScopeHookKind, span: SpanInfo) -> String {
    format!("{HOOK_ITEM_PREFIX}_{host}_{}_{}", kind_name(kind), span.start)
}

/// Name of the argument type that carries the injected parameters of that hook.
pub fn hook_arguments_name(host: &str, kind: ScopeHookKind, span: SpanInfo) -> String {
    format!("{HOOK_ITEM_PREFIX}Args_{host}_{}_{}", kind_name(kind), span.start)
}

/// Name of the value type whose fields are the parameters of `host`.
pub fn host_record_name(host: &str) -> String {
    format!("{HOOK_ITEM_PREFIX}HostArgs_{host}")
}

/// Name of the value type whose fields are the parameters of `scope` in `host`.
pub fn scope_record_name(host: &str, scope: &str) -> String {
    format!("{HOOK_ITEM_PREFIX}ScopeArgs_{host}_{scope}")
}

/// Name of the local that holds the arguments of the `launch` statement at `span`.
pub fn launch_arguments_local(span: SpanInfo) -> String {
    format!("{LAUNCH_ARGUMENTS_LOCAL_PREFIX}{}", span.start)
}

/// Name of the local that holds the arguments of the `with` statement at `span`.
pub fn scope_arguments_local(span: SpanInfo) -> String {
    format!("{SCOPE_ARGUMENTS_LOCAL_PREFIX}{}", span.start)
}

fn kind_name(kind: ScopeHookKind) -> &'static str {
    match kind {
        ScopeHookKind::Startup => "startup",
        ScopeHookKind::Init => "init",
        ScopeHookKind::Dispose => "dispose",
    }
}

/// The parameters of one host of the unit and of its scopes (at any depth), by name.
#[derive(Default)]
struct HostSignature {
    parameters: Vec<Spanned<Parameter>>,
    scopes: HashMap<String, Vec<Spanned<Parameter>>>,
}

fn host_signatures(items: &[Spanned<Node>]) -> HashMap<String, HostSignature> {
    fn scopes(body: &[Spanned<HostBodyItem>], found: &mut HashMap<String, Vec<Spanned<Parameter>>>) {
        for item in body {
            if let HostBodyItem::Scope(scope) = &item.node {
                found.insert(scope.node.name.node.name.clone(), scope.node.parameters.clone());
                scopes(&scope.node.body, found);
            }
        }
    }
    items
        .iter()
        .filter_map(|item| match &item.node {
            Node::HostDefinition(host) => {
                let mut signature =
                    HostSignature { parameters: host.node.parameters.clone(), scopes: HashMap::new() };
                scopes(&host.node.body, &mut signature.scopes);
                Some((host.node.name.node.name.clone(), signature))
            }
            _ => None,
        })
        .collect()
}

/// A host or scope record: its generated type and the parameters that are its fields.
struct Record {
    type_name: String,
    parameters: Vec<Spanned<Parameter>>,
}

/// Move every top-level host's hook bodies into generated items, generate the host and scope
/// records, and rewrite the unit's `launch`/`with` sites to build them. Idempotent: a generated
/// item that already exists is left alone, and a rewritten site has no arguments left.
pub fn lower_host_hooks(program: Spanned<Program>) -> Spanned<Program> {
    let span = program.span;
    let Program { mut items, leading_docs } = program.node;
    let existing = items
        .iter()
        .filter_map(|item| match &item.node {
            Node::Function(function) => Some(function.node.name.node.name.clone()),
            Node::TypeDefinition(definition) => Some(definition.node.name.node.name.clone()),
            _ => None,
        })
        .filter(|name| is_hook_item_name(name))
        .collect::<HashSet<_>>();
    let signatures = host_signatures(&items);
    let mut generated = Vec::new();
    for item in &mut items {
        if let Node::HostDefinition(host) = &mut item.node {
            let host_name = host.node.name.node.name.clone();
            let record = (!host.node.parameters.is_empty())
                .then(|| Record { type_name: host_record_name(&host_name), parameters: host.node.parameters.clone() });
            if let Some(record) = &record
                && !existing.contains(&record.type_name)
            {
                generated.push(record_type(record, host.span));
            }
            lower_body(&host_name, &mut host.node.body, true, record.as_ref(), &existing, &mut generated);
        }
    }
    for item in &mut items {
        rewrite_item_sites(item, &signatures);
    }
    items.extend(generated);
    Spanned::new(Program { items, leading_docs }, span)
}

fn lower_body(
    host: &str,
    body: &mut Vec<Spanned<HostBodyItem>>,
    global: bool,
    record: Option<&Record>,
    existing: &HashSet<String>,
    generated: &mut Vec<Spanned<Node>>,
) {
    let mut registrations = Vec::new();
    for item in body.iter_mut() {
        match &mut item.node {
            HostBodyItem::Scope(scope) => {
                let scope_name = scope.node.name.node.name.clone();
                // A scope's hooks see only that scope's own parameters.
                let scope_record = (!scope.node.parameters.is_empty()).then(|| Record {
                    type_name: scope_record_name(host, &scope_name),
                    parameters: scope.node.parameters.clone(),
                });
                if let Some(scope_record) = &scope_record
                    && !existing.contains(&scope_record.type_name)
                {
                    generated.push(record_type(scope_record, scope.span));
                }
                lower_body(host, &mut scope.node.body, false, scope_record.as_ref(), existing, generated);
            }
            HostBodyItem::Hook(hook) => {
                let function_name = hook_function_name(host, hook.node.kind, hook.span);
                if existing.contains(&function_name) {
                    continue;
                }
                if let Some(registration) = lower_hook(host, hook, function_name, global, record, generated) {
                    registrations.push(registration);
                }
            }
            HostBodyItem::Registry(_) | HostBodyItem::Registration(_) => {}
        }
    }
    body.extend(registrations);
}

fn identifier(name: impl Into<String>, span: SpanInfo) -> Spanned<Identifier> {
    Spanned::new(Identifier { name: name.into() }, span)
}

fn single_path(name: &str, span: SpanInfo) -> Spanned<Path> {
    Spanned::new(
        Path { segments: vec![Spanned::new(PathSegment { name: identifier(name, span), type_args: Vec::new() }, span)] },
        span,
    )
}

/// The private value type whose public fields are the record's parameters, in order.
fn record_type(record: &Record, span: SpanInfo) -> Spanned<Node> {
    let fields = record
        .parameters
        .iter()
        .map(|parameter| {
            Spanned::new(
                Field {
                    attributes: Vec::new(),
                    visibility: Spanned::new(Visibility::Public, parameter.span),
                    kind: FieldKind::Value,
                    event_capacity: None,
                    inject_qualifier: None,
                    name: parameter.node.name.clone(),
                    ty: parameter.node.ty.clone(),
                },
                parameter.span,
            )
        })
        .collect::<Vec<_>>();
    let definition = TypeDefinition {
        attributes: Vec::new(),
        visibility: Spanned::new(Visibility::Private, span),
        name: identifier(record.type_name.clone(), span),
        generics: Vec::new(),
        conformances: Vec::new(),
        field_docs: vec![None; fields.len()],
        fields,
        methods: Vec::new(),
        method_docs: Vec::new(),
        associated_type_bindings: Vec::new(),
    };
    Spanned::new(Node::TypeDefinition(Spanned::new(definition, span)), span)
}

/// Whether `body` names `name` as a single-segment path anywhere (a superset of its reads).
fn mentions(body: &Spanned<Block>, name: &str) -> bool {
    crate::syntax_query::Query::from(&body.node).of::<PathExpression>().any(|path| {
        matches!(path.path.node.segments.as_slice(), [segment] if segment.node.name.node.name == name)
    })
}

/// `T name = <object>.name;` for each of `parameters` that `body` names.
fn read_prelude(object: &str, parameters: &[Spanned<Parameter>], body: &Spanned<Block>) -> Vec<Spanned<Statement>> {
    parameters
        .iter()
        .filter(|parameter| mentions(body, &parameter.node.name.node.name))
        .map(|parameter| {
            let read = Expression::Member(Spanned::new(
                MemberExpression {
                    target: Box::new(Spanned::new(
                        Expression::Path(Spanned::new(
                            PathExpression { path: single_path(object, parameter.span) },
                            parameter.span,
                        )),
                        parameter.span,
                    )),
                    member: parameter.node.name.clone(),
                    type_args: Vec::new(),
                },
                parameter.span,
            ));
            let local = LetStatement {
                attributes: Vec::new(),
                mutable: parameter.node.mutable,
                name: parameter.node.name.clone(),
                type_annotation: Some(parameter.node.ty.clone()),
                value: Spanned::new(read, parameter.span),
            };
            Spanned::new(Statement::Let(Spanned::new(local, parameter.span)), parameter.span)
        })
        .collect()
}

fn object_parameter(name: &str, type_name: &str, span: SpanInfo) -> Spanned<Parameter> {
    Spanned::new(
        Parameter {
            attributes: Vec::new(),
            bulk: false,
            mutable: false,
            movable: false,
            name: identifier(name, span),
            ty: Spanned::new(Type::Complex(single_path(type_name, span)), span),
        },
        span,
    )
}

/// Generate the function (and argument type) for one hook; returns the argument type's
/// registration when the hook has injected parameters.
fn lower_hook(
    host: &str,
    hook: &mut Spanned<ScopeHook>,
    function_name: String,
    global: bool,
    record: Option<&Record>,
    generated: &mut Vec<Spanned<Node>>,
) -> Option<Spanned<HostBodyItem>> {
    let span = hook.span;
    let parameters = std::mem::take(&mut hook.node.parameters);
    let body_span = hook.node.body.span;
    let mut body = std::mem::replace(&mut hook.node.body, Spanned::new(Block { statements: Vec::new() }, body_span));
    let arguments_name = hook_arguments_name(host, hook.node.kind, span);
    let mut function_parameters = Vec::with_capacity(2);
    let mut prelude = Vec::new();
    if let Some(record) = record {
        function_parameters.push(object_parameter(ARGUMENTS_RECORD_PARAMETER, &record.type_name, span));
        prelude.extend(read_prelude(ARGUMENTS_RECORD_PARAMETER, &record.parameters, &body));
    }
    if !parameters.is_empty() {
        function_parameters.push(object_parameter(SERVICES_PARAMETER, &arguments_name, span));
        prelude.extend(read_prelude(SERVICES_PARAMETER, &parameters, &body));
    }
    body.node.statements.splice(0..0, prelude);
    let function = FunctionDefinition {
        attributes: Vec::new(),
        visibility: Spanned::new(Visibility::Private, span),
        name: identifier(function_name, span),
        generics: Vec::new(),
        where_bounds: Vec::new(),
        parameter_docs: vec![None; function_parameters.len()],
        parameters: function_parameters,
        return_type: Some(Spanned::new(Type::Primitive(Spanned::new(PrimitiveType::Unit, span)), span)),
        body,
    };
    generated.push(Spanned::new(Node::Function(Spanned::new(function, span)), span));
    if parameters.is_empty() {
        return None;
    }

    // Public fields: the hook function, outside the type, reads them.
    let fields = parameters
        .iter()
        .map(|parameter| {
            Spanned::new(
                Field {
                    attributes: Vec::new(),
                    visibility: Spanned::new(Visibility::Public, parameter.span),
                    kind: FieldKind::Injected,
                    event_capacity: None,
                    inject_qualifier: None,
                    name: parameter.node.name.clone(),
                    ty: parameter.node.ty.clone(),
                },
                parameter.span,
            )
        })
        .collect::<Vec<_>>();
    let arguments = TypeDefinition {
        attributes: Vec::new(),
        visibility: Spanned::new(Visibility::Private, span),
        name: identifier(arguments_name.clone(), span),
        generics: Vec::new(),
        conformances: Vec::new(),
        field_docs: vec![None; fields.len()],
        fields,
        methods: Vec::new(),
        method_docs: Vec::new(),
        associated_type_bindings: Vec::new(),
    };
    generated.push(Spanned::new(Node::TypeDefinition(Spanned::new(arguments, span)), span));

    let entry = RegistryEntry {
        // Global arguments live for the launch; a named scope's arguments are per activation.
        lifetime: global.then_some(RegistrationLifetime::Single),
        implementation: single_path(&arguments_name, span),
        target: None,
    };
    Some(Spanned::new(HostBodyItem::Registration(Spanned::new(entry, span)), span))
}

/// Rewrite the `launch` and `with` sites in the statement bodies of one item.
fn rewrite_item_sites(item: &mut Spanned<Node>, signatures: &HashMap<String, HostSignature>) {
    match &mut item.node {
        Node::Function(function) => rewrite_statements(&mut function.node.body.node.statements, signatures, None),
        Node::Method(method) => rewrite_statements(&mut method.node.body.node.statements, signatures, None),
        Node::TestDefinition(test) => rewrite_statements(&mut test.node.statements, signatures, None),
        Node::InlineModule(module) => {
            for nested in &mut module.node.items {
                rewrite_item_sites(nested, signatures);
            }
        }
        _ => {}
    }
}

/// `T local = T { p1: a1, p2: a2 };` for the record `type_name` built from `arguments`.
fn record_local(
    type_name: &str,
    local: String,
    parameters: &[Spanned<Parameter>],
    arguments: Vec<Spanned<Expression>>,
    span: SpanInfo,
) -> Spanned<Statement> {
    let fields = parameters
        .iter()
        .zip(arguments)
        .map(|(parameter, argument)| {
            // The field carries the argument's own span, so a mismatch is reported at it.
            let argument_span = argument.span;
            Spanned::new(
                StructLiteralField {
                    name: identifier(parameter.node.name.node.name.clone(), argument_span),
                    value: argument,
                },
                argument_span,
            )
        })
        .collect();
    let value = Expression::StructLiteral(Spanned::new(
        StructLiteralExpression { path: single_path(type_name, span), fields },
        span,
    ));
    let statement = LetStatement {
        attributes: Vec::new(),
        mutable: false,
        name: identifier(local, span),
        type_annotation: Some(Spanned::new(Type::Complex(single_path(type_name, span)), span)),
        value: Spanned::new(value, span),
    };
    Spanned::new(Statement::Let(Spanned::new(statement, span)), span)
}

/// Insert the record local before each `launch`/`with` whose argument count matches the
/// declaration and clear the statement's arguments. `launched` is the host that an earlier
/// `launch` of the same body launched; a `with` names a scope of that host.
fn rewrite_statements(
    statements: &mut Vec<Spanned<Statement>>,
    signatures: &HashMap<String, HostSignature>,
    mut launched: Option<String>,
) {
    let mut index = 0;
    while index < statements.len() {
        let mut inserted = None;
        match &mut statements[index].node {
            Statement::Launch(launch) => {
                let host = launch.node.host_path.node.segments.last().map(|segment| segment.node.name.node.name.clone());
                launched = host.filter(|host| signatures.contains_key(host));
                if let Some(host) = &launched {
                    let parameters = &signatures[host].parameters;
                    if !launch.node.arguments.is_empty() && launch.node.arguments.len() == parameters.len() {
                        let arguments = std::mem::take(&mut launch.node.arguments);
                        inserted = Some(record_local(
                            &host_record_name(host),
                            launch_arguments_local(launch.span),
                            parameters,
                            arguments,
                            launch.span,
                        ));
                    }
                }
            }
            Statement::With(with) => {
                let scope = &with.node.scope_name.node.name;
                if let Some(host) = &launched
                    && let Some(parameters) = signatures[host].scopes.get(scope)
                    && !with.node.arguments.is_empty()
                    && with.node.arguments.len() == parameters.len()
                {
                    let arguments = std::mem::take(&mut with.node.arguments);
                    inserted = Some(record_local(
                        &scope_record_name(host, scope),
                        scope_arguments_local(with.span),
                        parameters,
                        arguments,
                        with.span,
                    ));
                }
                rewrite_statements(&mut with.node.body.node.statements, signatures, launched.clone());
            }
            Statement::If(if_statement) => {
                rewrite_statements(&mut if_statement.node.then_block.node.statements, signatures, launched.clone());
                let mut branch = if_statement.node.else_branch.as_mut();
                while let Some(current) = branch {
                    match &mut current.node {
                        ElseBranch::Block(block) => {
                            rewrite_statements(&mut block.node.statements, signatures, launched.clone());
                            branch = None;
                        }
                        ElseBranch::If(nested) => {
                            rewrite_statements(&mut nested.node.then_block.node.statements, signatures, launched.clone());
                            branch = nested.node.else_branch.as_mut();
                        }
                    }
                }
            }
            Statement::While(while_statement) => {
                rewrite_statements(&mut while_statement.node.body.node.statements, signatures, launched.clone());
            }
            Statement::For(for_statement) => {
                rewrite_statements(&mut for_statement.node.body.node.statements, signatures, launched.clone());
            }
            Statement::Use(scoped) => {
                if let Some(body) = &mut scoped.node.body {
                    rewrite_statements(&mut body.node.statements, signatures, launched.clone());
                }
            }
            Statement::Let(_)
            | Statement::Return(_)
            | Statement::Break(_)
            | Statement::Continue(_)
            | Statement::Expression(_) => {}
        }
        match inserted {
            Some(statement) => {
                statements.insert(index, statement);
                index += 2;
            }
            None => index += 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::parse_program;

    const SOURCE: &str = r#"
type Greeter {}
host AppHost()
{
    registry { single Greeter; }
    startup(Greeter greeter) { }
    scope Visit()
    {
        init() { }
        dispose(Greeter greeter) { }
    }
}
i32 Main() { launch AppHost(); return 0; }
"#;

    fn names(program: &Spanned<Program>) -> Vec<String> {
        program
            .node
            .items
            .iter()
            .filter_map(|item| match &item.node {
                Node::Function(function) => Some(function.node.name.node.name.clone()),
                Node::TypeDefinition(definition) => Some(definition.node.name.node.name.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn hooks_become_functions_and_argument_types_once() {
        let program = parse_program(SOURCE).expect("parse");
        let lowered = lower_host_hooks(program);
        let generated = names(&lowered).into_iter().filter(|name| is_hook_item_name(name)).collect::<Vec<_>>();
        // startup and dispose have parameters (function + argument type); init has none.
        assert_eq!(generated.len(), 5, "{generated:?}");
        assert!(generated.iter().all(|name| name.starts_with("__BeskidHook")));
        let again = lower_host_hooks(lowered.clone());
        assert_eq!(again, lowered, "lowering is idempotent");
    }

    #[test]
    fn argument_types_register_in_the_hook_container_level() {
        let program = lower_host_hooks(parse_program(SOURCE).expect("parse"));
        let types = crate::composition::types::test_types::DeclaredTypes::new(&program, &[]);
        let result = crate::composition::resolve_composition(crate::composition::CompositionInput {
            program: &program,
            is_mod_project: false,
            types: &types,
        });
        let arguments = result
            .snapshot
            .registrations
            .iter()
            .filter(|registration| is_hook_item_name(&registration.implementation))
            .collect::<Vec<_>>();
        assert_eq!(arguments.len(), 2);
        let global = arguments.iter().filter(|entry| entry.scope_id == crate::composition::ScopeId::GLOBAL).count();
        assert_eq!(global, 1, "startup arguments live at global scope");
        let scoped = arguments.iter().find(|entry| entry.scope_id != crate::composition::ScopeId::GLOBAL).unwrap();
        assert_eq!(scoped.lifetime, crate::composition::RegistrationLifetime::Scoped);
        assert!(result.issues.is_empty(), "{:?}", result.issues);
    }
    const PARAMETERS: &str = r#"
type Greeter {}
host AppHost(string name, i32 unused)
{
    registry { single Greeter; }
    startup(Greeter greeter) { Say(name); }
    scope Visit(i32 id)
    {
        init() { Note(id); }
        dispose() { }
    }
}
unit Say(string text) { }
unit Note(i32 id) { }
i32 Main()
{
    launch AppHost("Ada", 1);
    with Visit(7) { }
    return 0;
}
"#;

    fn function<'a>(program: &'a Spanned<Program>, prefix: &str) -> &'a FunctionDefinition {
        program
            .node
            .items
            .iter()
            .find_map(|item| match &item.node {
                Node::Function(function) if function.node.name.node.name.starts_with(prefix) => Some(&function.node),
                _ => None,
            })
            .unwrap_or_else(|| panic!("generated function {prefix}"))
    }

    #[test]
    fn host_and_scope_parameters_become_records_built_at_the_site() {
        let lowered = lower_host_hooks(parse_program(PARAMETERS).expect("parse"));
        let generated = names(&lowered);
        assert!(generated.contains(&host_record_name("AppHost")), "{generated:?}");
        assert!(generated.contains(&scope_record_name("AppHost", "Visit")), "{generated:?}");
        let main = function(&lowered, "Main");
        let statements = &main.body.node.statements;
        let Statement::Let(launch_record) = &statements[0].node else { panic!("record local before launch") };
        assert!(launch_record.node.name.node.name.starts_with(LAUNCH_ARGUMENTS_LOCAL_PREFIX));
        let Expression::StructLiteral(literal) = &launch_record.node.value.node else { panic!("record literal") };
        assert_eq!(literal.node.fields.len(), 2);
        assert_eq!(literal.node.fields[0].node.name.node.name, "name");
        let Statement::Launch(launch) = &statements[1].node else { panic!("launch") };
        assert!(launch.node.arguments.is_empty(), "the arguments moved into the record");
        let Statement::Let(scope_record) = &statements[2].node else { panic!("record local before with") };
        assert!(scope_record.node.name.node.name.starts_with(SCOPE_ARGUMENTS_LOCAL_PREFIX));
        let Statement::With(with) = &statements[3].node else { panic!("with") };
        assert!(with.node.arguments.is_empty());

        let startup = function(&lowered, &format!("{HOOK_ITEM_PREFIX}_AppHost_startup_"));
        assert_eq!(startup.parameters.len(), 2, "record first, then the argument object");
        assert_eq!(startup.parameters[1].node.name.node.name, SERVICES_PARAMETER);
        assert_eq!(startup.parameters[0].node.name.node.name, ARGUMENTS_RECORD_PARAMETER);
        // Only the parameter the body names is read from the record.
        let Statement::Let(read) = &startup.body.node.statements[0].node else { panic!("record read") };
        assert_eq!(read.node.name.node.name, "name");
        assert_eq!(startup.body.node.statements.len(), 2);
        let init = function(&lowered, &format!("{HOOK_ITEM_PREFIX}_AppHost_init_"));
        assert_eq!(init.parameters.len(), 1, "a scope hook takes its scope record");
        let dispose = function(&lowered, &format!("{HOOK_ITEM_PREFIX}_AppHost_dispose_"));
        assert_eq!(dispose.parameters.len(), 1);
        assert!(dispose.body.node.statements.is_empty(), "nothing named, nothing read");

        assert_eq!(lower_host_hooks(lowered.clone()), lowered, "lowering with records is idempotent");
    }

    #[test]
    fn a_mismatched_argument_count_is_left_for_composition() {
        let source = PARAMETERS.replace("launch AppHost(\"Ada\", 1);", "launch AppHost(\"Ada\");");
        let lowered = lower_host_hooks(parse_program(&source).expect("parse"));
        let main = function(&lowered, "Main");
        let Statement::Launch(launch) = &main.body.node.statements[0].node else { panic!("launch stays first") };
        assert_eq!(launch.node.arguments.len(), 1);
        let types = crate::composition::types::test_types::DeclaredTypes::new(&lowered, &[]);
        let result = crate::composition::resolve_composition(crate::composition::CompositionInput {
            program: &lowered,
            is_mod_project: false,
            types: &types,
        });
        assert!(
            result.issues.iter().any(|issue| matches!(
                issue,
                crate::composition::CompositionIssue::LaunchArgumentCount { expected: 2, found: 1, .. }
            )),
            "{:?}",
            result.issues
        );
    }

    #[test]
    fn hooks_of_a_host_with_parameters_take_the_record() {
        let lowered = lower_host_hooks(parse_program(PARAMETERS).expect("parse"));
        let types = crate::composition::types::test_types::DeclaredTypes::new(&lowered, &[]);
        let result = crate::composition::resolve_composition(crate::composition::CompositionInput {
            program: &lowered,
            is_mod_project: false,
            types: &types,
        });
        assert!(result.issues.is_empty(), "{:?}", result.issues);
        assert!(result.plan.startup_hooks.iter().all(|hook| hook.takes_arguments_record));
        assert!(result.plan.init_hooks.iter().all(|hook| hook.takes_arguments_record));
        assert!(result.plan.disposal_hooks.iter().all(|hook| hook.takes_arguments_record));
    }
}
