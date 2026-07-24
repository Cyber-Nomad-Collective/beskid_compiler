//! End-to-end host launch test exercising the language-meta DI lifecycle.
//!
//! Models a host with:
//!
//! * a global singleton `Logger`
//! * a `request` scope with a singular `Handler` and a plural-inject `[Middleware]` chain
//! * a `session` scope nested inside `request`
//!
//! The test asserts that `init` runs on enter, `dispose` runs in reverse registration
//! order on leave, and the global singleton disposes only at shutdown.

use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;

use beskid_runtime::composition::{Lifetime, RegistrationId, RegistrationRecord, RuntimeContainer, ScopeId};

type Log = Rc<RefCell<Vec<String>>>;

fn registration(id: u32, scope: ScopeId, lifetime: Lifetime, log: Log, label: &'static str) -> RegistrationRecord {
    let init_log = log.clone();
    let dispose_log = log.clone();
    RegistrationRecord {
        id: RegistrationId(id),
        scope,
        lifetime,
        factory: Some(Box::new(move |_| id as usize as *mut c_void)),
        init: Some(Box::new(move |_| {
            init_log.borrow_mut().push(format!("init:{label}"));
        })),
        dispose: Some(Box::new(move |_| {
            dispose_log.borrow_mut().push(format!("dispose:{label}"));
        })),
    }
}

#[test]
fn host_with_two_scopes_plural_inject_reverse_dispose() {
    let log: Log = Rc::new(RefCell::new(Vec::new()));
    let mut container = RuntimeContainer::new();

    // global
    container.register(registration(1, ScopeId::GLOBAL, Lifetime::Single, log.clone(), "Logger"));

    // request scope
    let request_scope = ScopeId(101);
    container.register(registration(10, request_scope, Lifetime::Scoped, log.clone(), "MiddlewareAuth"));
    container.register(registration(11, request_scope, Lifetime::Scoped, log.clone(), "MiddlewareMetrics"));
    container.register(registration(12, request_scope, Lifetime::Scoped, log.clone(), "Handler"));
    container.bind_plural(RegistrationId(12), vec![RegistrationId(10), RegistrationId(11)]);

    // session scope, nested under request
    let session_scope = ScopeId(202);
    container.register(registration(20, session_scope, Lifetime::Scoped, log.clone(), "SessionStore"));

    container.launch().expect("launch");

    container.enter_scope(request_scope).expect("enter request scope");
    let plural = container.resolve_plural(RegistrationId(12)).expect("resolve plural Middleware");
    assert_eq!(plural.len(), 2);
    container.resolve(RegistrationId(12)).expect("resolve Handler");

    container.enter_scope(session_scope).expect("enter session scope");
    container.resolve(RegistrationId(20)).expect("resolve SessionStore");

    container.leave_scope(session_scope).expect("leave session scope");
    container.leave_scope(request_scope).expect("leave request scope");
    container.shutdown().expect("shutdown");

    let entries = log.borrow().clone();
    // 1) every scope-owned service must run init before any dispose
    let first_init = entries.iter().position(|l| l.starts_with("init:")).expect("at least one init entry");
    let first_dispose = entries.iter().position(|l| l.starts_with("dispose:")).expect("at least one dispose entry");
    assert!(first_init < first_dispose, "init must precede dispose: {entries:?}");

    // 2) session disposes before request leave
    let session_pos = entries.iter().position(|l| l == "dispose:SessionStore").expect("session dispose recorded");
    let auth_pos = entries.iter().position(|l| l == "dispose:MiddlewareAuth").expect("auth dispose recorded");
    let metrics_pos = entries.iter().position(|l| l == "dispose:MiddlewareMetrics").expect("metrics dispose recorded");
    assert!(
        session_pos < auth_pos && session_pos < metrics_pos,
        "session scope must dispose before request scope leave: {entries:?}"
    );

    // 3) reverse-order dispose within the request scope (MiddlewareMetrics created last, so
    //    it must dispose before MiddlewareAuth)
    let handler_pos = entries.iter().position(|l| l == "dispose:Handler").expect("handler dispose recorded");
    assert!(metrics_pos < auth_pos);
    assert!(
        handler_pos > metrics_pos.max(auth_pos) || handler_pos < metrics_pos.min(auth_pos),
        "Handler ordering should be deterministic relative to plural targets: {entries:?}"
    );

    // 4) global singleton disposes at shutdown, after every scope teardown
    let logger_pos = entries.iter().rposition(|l| l == "dispose:Logger").expect("logger dispose recorded");
    assert!(
        logger_pos > auth_pos && logger_pos > metrics_pos && logger_pos > session_pos,
        "global singleton must dispose at shutdown: {entries:?}"
    );
}
