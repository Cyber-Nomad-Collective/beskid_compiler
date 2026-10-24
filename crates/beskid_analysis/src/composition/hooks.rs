//! Host hooks (`startup`, `init`, `dispose`) as ordinary items of the expanded syntax.
//!
//! A hook body is executable code with injected parameters. Instead of a second, hook-only
//! semantic path, expansion moves each hook body into a private top-level function and, when
//! the hook has parameters, a private argument type whose injected fields mirror the parameters
//! one to one. The argument type is registered in the hook's own container level, so the
//! ordinary composition graph resolves, orders and diagnoses every hook parameter exactly like a
//! field `inject`. Lowering calls the function with the argument object's fields.
//!
//! The hook node stays in the host with its span (the composition plan keys hooks by it); its
//! parameters and body are emptied so that each source node exists exactly once in the tree.

use crate::syntax::{
    Block, Field, FieldKind, FunctionDefinition, HostBodyItem, Identifier, Node, Path, PathSegment, PrimitiveType,
    Program, RegistrationLifetime, RegistryEntry, ScopeHook, ScopeHookKind, SpanInfo, Spanned, Type, TypeDefinition,
    Visibility,
};

/// Reserved name prefix of compiler-generated hook items. Source never names them.
pub const HOOK_ITEM_PREFIX: &str = "__BeskidHook";

/// Whether `name` is a compiler-generated hook function or hook argument type.
pub fn is_hook_item_name(name: &str) -> bool {
    name.starts_with(HOOK_ITEM_PREFIX)
}

/// Name of the function that holds the body of the hook declared at `span` in `host`.
pub fn hook_function_name(host: &str, kind: ScopeHookKind, span: SpanInfo) -> String {
    format!("{HOOK_ITEM_PREFIX}_{host}_{}_{}", kind_name(kind), span.start)
}

/// Name of the argument type that carries the injected parameters of that hook.
pub fn hook_arguments_name(host: &str, kind: ScopeHookKind, span: SpanInfo) -> String {
    format!("{HOOK_ITEM_PREFIX}Args_{host}_{}_{}", kind_name(kind), span.start)
}

fn kind_name(kind: ScopeHookKind) -> &'static str {
    match kind {
        ScopeHookKind::Startup => "startup",
        ScopeHookKind::Init => "init",
        ScopeHookKind::Dispose => "dispose",
    }
}

/// Move every top-level host's hook bodies into generated items. Idempotent: a hook whose
/// function already exists is left alone.
pub fn lower_host_hooks(program: Spanned<Program>) -> Spanned<Program> {
    let span = program.span;
    let Program { mut items, leading_docs } = program.node;
    let existing = items
        .iter()
        .filter_map(|item| match &item.node {
            Node::Function(function) => Some(function.node.name.node.name.clone()),
            _ => None,
        })
        .filter(|name| is_hook_item_name(name))
        .collect::<std::collections::HashSet<_>>();
    let mut generated = Vec::new();
    for item in &mut items {
        if let Node::HostDefinition(host) = &mut item.node {
            let host_name = host.node.name.node.name.clone();
            lower_body(&host_name, &mut host.node.body, true, &existing, &mut generated);
        }
    }
    items.extend(generated);
    Spanned::new(Program { items, leading_docs }, span)
}

fn lower_body(
    host: &str,
    body: &mut Vec<Spanned<HostBodyItem>>,
    global: bool,
    existing: &std::collections::HashSet<String>,
    generated: &mut Vec<Spanned<Node>>,
) {
    let mut registrations = Vec::new();
    for item in body.iter_mut() {
        match &mut item.node {
            HostBodyItem::Scope(scope) => lower_body(host, &mut scope.node.body, false, existing, generated),
            HostBodyItem::Hook(hook) => {
                let function_name = hook_function_name(host, hook.node.kind, hook.span);
                if existing.contains(&function_name) {
                    continue;
                }
                if let Some(registration) = lower_hook(host, hook, function_name, global, generated) {
                    registrations.push(registration);
                }
            }
            HostBodyItem::Registry(_) | HostBodyItem::Registration(_) => {}
        }
    }
    body.extend(registrations);
}

/// Generate the function (and argument type) for one hook; returns the argument type's
/// registration when the hook has parameters.
fn lower_hook(
    host: &str,
    hook: &mut Spanned<ScopeHook>,
    function_name: String,
    global: bool,
    generated: &mut Vec<Spanned<Node>>,
) -> Option<Spanned<HostBodyItem>> {
    let span = hook.span;
    let parameters = std::mem::take(&mut hook.node.parameters);
    let body_span = hook.node.body.span;
    let body = std::mem::replace(&mut hook.node.body, Spanned::new(Block { statements: Vec::new() }, body_span));
    let function = FunctionDefinition {
        attributes: Vec::new(),
        visibility: Spanned::new(Visibility::Private, span),
        name: Spanned::new(Identifier { name: function_name }, span),
        generics: Vec::new(),
        where_bounds: Vec::new(),
        parameter_docs: vec![None; parameters.len()],
        parameters: parameters.clone(),
        return_type: Some(Spanned::new(Type::Primitive(Spanned::new(PrimitiveType::Unit, span)), span)),
        body,
    };
    generated.push(Spanned::new(Node::Function(Spanned::new(function, span)), span));
    if parameters.is_empty() {
        return None;
    }

    let arguments_name = hook_arguments_name(host, hook.node.kind, span);
    let fields = parameters
        .iter()
        .map(|parameter| {
            Spanned::new(
                Field {
                    attributes: Vec::new(),
                    visibility: Spanned::new(Visibility::Private, parameter.span),
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
        name: Spanned::new(Identifier { name: arguments_name.clone() }, span),
        generics: Vec::new(),
        conformances: Vec::new(),
        field_docs: vec![None; fields.len()],
        fields,
        methods: Vec::new(),
        method_docs: Vec::new(),
        associated_type_bindings: Vec::new(),
    };
    generated.push(Spanned::new(Node::TypeDefinition(Spanned::new(arguments, span)), span));

    let path = Path {
        segments: vec![Spanned::new(
            PathSegment { name: Spanned::new(Identifier { name: arguments_name }, span), type_args: Vec::new() },
            span,
        )],
    };
    let entry = RegistryEntry {
        // Global arguments live for the launch; a named scope's arguments are per activation.
        lifetime: global.then_some(RegistrationLifetime::Single),
        implementation: Spanned::new(path, span),
        target: None,
    };
    Some(Spanned::new(HostBodyItem::Registration(Spanned::new(entry, span)), span))
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
        let result = crate::composition::resolve_composition(crate::composition::CompositionInput {
            program: &program,
            is_mod_project: false,
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
}
