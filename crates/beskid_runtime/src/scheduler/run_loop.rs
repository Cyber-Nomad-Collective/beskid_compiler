use crate::gc::with_current_root_if_active;

use super::state::{FiberState, JoinOutcome, id_to_key_unchecked};
use super::tls;

pub fn park_current(add_wait: impl FnOnce(super::FiberKey)) {
    let key = tls::current_fiber_key().expect("park outside fiber");
    add_wait(key);
    tls::set_fiber_state(key, FiberState::Parked);
    tls::set_parking(true);
    fiber_yield();
    tls::set_parking(false);
    tls::set_fiber_state(key, FiberState::Running);
}

pub fn fiber_yield() {
    if !tls::in_fiber_scheduler() {
        std::thread::yield_now();
        return;
    }
    if let Some(y) = tls::yielder() {
        unsafe {
            (&*y).suspend(());
        }
    }
}

pub fn fiber_join(id: i64, out: *mut i64) -> i64 {
    let key = id_to_key_unchecked(id);
    loop {
        if let Some(code) = tls::join_status_from_snapshot(key, out) {
            return code;
        }
        park_current(|_| {});
    }
}

/// Run `main` on fiber 0 and drive the scheduler until quiescence.
pub fn run_main_fiber(main: extern "C" fn(*mut u8) -> i64, env: *mut u8) -> i64 {
    if let Some(s) = tls::take_scheduler() {
        std::mem::forget(s);
    }
    let main_key = tls::with_scheduler(|s| {
        let main_key = s.main_fiber;
        s.fibers.get_mut(main_key).expect("main").start(main, env);
        if !s.run_queue.contains(&main_key) {
            s.run_queue.push_back(main_key);
        }
        main_key
    });

    tls::set_in_scheduler(true);
    loop {
        let (continue_running, progressed, blocked) = tls::with_scheduler(|s| {
            if !s.should_continue() {
                return (false, false, false);
            }
            let progressed = s.run_one_step();
            let blocked = !progressed && s.all_blocked();
            (true, progressed, blocked)
        });
        if !continue_running {
            break;
        }
        if !progressed {
            if blocked {
                tls::with_scheduler(|s| {
                    if s.should_continue() {
                        panic!("scheduler deadlock: all fibers parked with an empty run queue");
                    }
                });
            }
            with_current_root_if_active(|root| {
                root.heap.collect();
                root.runtime_state.heap_live_bytes = root.heap.bytes_allocated();
            });
            std::thread::yield_now();
        }
    }
    tls::set_in_scheduler(false);

    let mut result = 0i64;
    tls::with_scheduler(|s| {
        s.join_non_detached_children();
        if let Some(JoinOutcome::Value(n)) = s.fibers.get(main_key).and_then(|f| f.outcome.as_ref()) {
            result = *n;
        }
        // Leak coroutine stacks on shutdown so Drop does not force-unwind across C unwind boundaries.
        for fiber in s.fibers.values_mut() {
            if let Some(coro) = fiber.coroutine.take() {
                std::mem::forget(coro);
            }
        }
    });
    if let Some(s) = tls::take_scheduler() {
        std::mem::forget(s);
    }
    result
}

/// Execute a Rust closure as the main fiber (for tests).
pub fn run_closure_as_main<F>(f: F) -> i64
where
    F: FnOnce() -> i64 + 'static,
{
    struct Ctx {
        func: Option<Box<dyn FnOnce() -> i64>>,
        panic: Option<Box<dyn std::any::Any + Send>>,
    }
    extern "C" fn trampoline(env: *mut u8) -> i64 {
        let ctx = unsafe { &mut *(env as *mut Ctx) };
        let func = ctx.func.take().expect("trampoline once");
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(func)) {
            Ok(value) => value,
            Err(panic) => {
                ctx.panic = Some(panic);
                0
            }
        }
    }
    let mut ctx = Ctx { func: Some(Box::new(f)), panic: None };
    let ptr = &mut ctx as *mut Ctx as *mut u8;
    let result = run_main_fiber(trampoline, ptr);
    if let Some(panic) = ctx.panic {
        std::panic::resume_unwind(panic);
    }
    result
}
